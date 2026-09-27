#!/usr/bin/env python3
"""Small deterministic worker for the Virt Extension Layer."""
from __future__ import annotations
import argparse, difflib, hashlib, json, os, platform, re, sys
from datetime import datetime, timezone
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
QUEUE = ROOT / "queue"
RESULTS = ROOT / "results"
STATE = ROOT / "state" / "index.json"
TOOLS = {"inventory", "grep", "sha256", "json_validate", "python_compile", "system_probe", "read_text", "artifact_manifest", "diff_text", "state_report", "cargo_check", "memory_boot", "audit_append", "audit_search", "memory_candidates", "memory_promote", "memory_revision"}

def now():
    return datetime.now(timezone.utc).isoformat()

def safe_path(value):
    p = (ROOT / value).resolve()
    if p != ROOT and ROOT not in p.parents:
        raise ValueError("path escapes repository root")
    if ".git" in p.relative_to(ROOT).parts:
        raise ValueError(".git is not allowed")
    return p

def inventory(args):
    base = safe_path(args.get("root", ".")); rows=[]
    for p in sorted(base.rglob("*")):
        if not p.is_file() or ".git" in p.parts: continue
        h=hashlib.sha256()
        with p.open("rb") as f:
            for b in iter(lambda:f.read(1024*1024), b""): h.update(b)
        rows.append({"path":p.relative_to(ROOT).as_posix(),"size":p.stat().st_size,"sha256":h.hexdigest()})
    return {"files":rows,"count":len(rows)}

def grep(args):
    pattern=args.get("pattern")
    if not isinstance(pattern,str) or not pattern: raise ValueError("grep requires args.pattern")
    rx=re.compile(pattern); root=safe_path(args.get("root",".")); limit=min(int(args.get("limit",100)),500); out=[]
    for p in sorted(root.rglob("*")):
        if len(out)>=limit or not p.is_file() or ".git" in p.parts: continue
        try: text=p.read_text(encoding="utf-8")
        except (UnicodeDecodeError,OSError): continue
        for n,line in enumerate(text.splitlines(),1):
            if rx.search(line): out.append({"path":p.relative_to(ROOT).as_posix(),"line":n,"text":line[:1000]})
            if len(out)>=limit: break
    return {"matches":out,"count":len(out),"truncated":len(out)>=limit}

def sha256(args):
    p=safe_path(args["path"]); h=hashlib.sha256()
    with p.open("rb") as f:
        for b in iter(lambda:f.read(1024*1024),b""): h.update(b)
    return {"path":p.relative_to(ROOT).as_posix(),"sha256":h.hexdigest(),"size":p.stat().st_size}

def json_validate(args):
    p=safe_path(args["path"])
    with p.open(encoding="utf-8") as f: obj=json.load(f)
    return {"path":p.relative_to(ROOT).as_posix(),"valid":True,"top_level":type(obj).__name__}

def python_compile(args):
    p=safe_path(args["path"])
    if p.suffix!=".py": raise ValueError("python_compile requires .py")
    compile(p.read_text(encoding="utf-8"),str(p),"exec")
    return {"path":p.relative_to(ROOT).as_posix(),"syntax_ok":True}

def system_probe(args):
    return {"platform":platform.platform(),"python":sys.version.split()[0],"machine":platform.machine(),"github_actions":os.environ.get("GITHUB_ACTIONS","false"),"runner_os":os.environ.get("RUNNER_OS")}

def read_text(args):
    p=safe_path(args["path"])
    limit=min(max(int(args.get("max_bytes",8192)),1),32768)
    data=p.read_bytes()[:limit]
    try:
        text=data.decode("utf-8")
    except UnicodeDecodeError:
        raise ValueError("file is not UTF-8 text")
    return {"path":p.relative_to(ROOT).as_posix(),"bytes_returned":len(data),"truncated":p.stat().st_size>limit,"text":text}

def diff_text(args):
    left=safe_path(args["left"])
    right=safe_path(args["right"])
    limit=min(max(int(args.get("max_lines",500)),1),2000)
    try:
        a=left.read_text(encoding="utf-8").splitlines()
        b=right.read_text(encoding="utf-8").splitlines()
    except UnicodeDecodeError:
        raise ValueError("diff_text requires UTF-8 text files")
    diff=list(difflib.unified_diff(a,b,fromfile=left.relative_to(ROOT).as_posix(),tofile=right.relative_to(ROOT).as_posix(),lineterm=""))
    return {"left":left.relative_to(ROOT).as_posix(),"right":right.relative_to(ROOT).as_posix(),"lines":diff[:limit],"count":len(diff),"truncated":len(diff)>limit}

def state_report(args):
    state=json.loads(STATE.read_text()) if STATE.exists() else {}
    queued=sorted(p.name for p in QUEUE.glob("*.json"))
    results=sorted(p.name for p in RESULTS.glob("*.json"))
    return {"state":state,"queued":queued,"result_files":results,"queue_count":len(queued),"result_count":len(results)}

def artifact_manifest(args):
    base=RESULTS
    rows=[]
    for p in sorted(base.glob("*.json")):
        h=hashlib.sha256(p.read_bytes()).hexdigest()
        rows.append({"path":p.relative_to(ROOT).as_posix(),"size":p.stat().st_size,"sha256":h})
    return {"results":rows,"count":len(rows)}

def cargo_check(args):
    manifest=safe_path(args.get("manifest","Cargo.toml"))
    if manifest.name!="Cargo.toml":
        raise ValueError("cargo_check requires Cargo.toml")
    if not manifest.exists():
        raise ValueError("Cargo.toml not found")
    import subprocess
    timeout=min(max(int(args.get("timeout",120)),1),300)
    proc=subprocess.run(["cargo","check","--manifest-path",str(manifest)],cwd=str(ROOT),capture_output=True,text=True,timeout=timeout)
    return {"manifest":manifest.relative_to(ROOT).as_posix(),"returncode":proc.returncode,"ok":proc.returncode==0,"stdout":proc.stdout[-12000:],"stderr":proc.stderr[-12000:]}


def audit_append(args):
    audit_path=ROOT / "state" / "AUDIT.jsonl"
    event={
        "schema":1,
        "event_id":str(args.get("event_id","")).strip(),
        "request_id":str(args.get("request_id","")).strip(),
        "kind":str(args.get("kind","request")).strip(),
        "timestamp":str(args.get("timestamp","")).strip() or now(),
        "actor":str(args.get("actor","Virt")).strip(),
        "summary":str(args.get("summary","")).strip(),
        "result":str(args.get("result","")).strip()
    }
    if not event["event_id"] or not event["summary"]:
        raise ValueError("audit_append requires event_id and summary")
    audit_path.parent.mkdir(exist_ok=True)
    with audit_path.open("a",encoding="utf-8") as f:
        f.write(json.dumps(event,ensure_ascii=False,separators=(",",":"))+"\n")
    return {"path":"state/AUDIT.jsonl","event":event}


def audit_search(args):
    audit_path=ROOT / "state" / "AUDIT.jsonl"
    if not audit_path.exists():
        return {"path":"state/AUDIT.jsonl","matches":[],"count":0,"truncated":False}
    query=str(args.get("query","")).strip().lower()
    request_id=str(args.get("request_id","")).strip()
    kind=str(args.get("kind","")).strip()
    since=str(args.get("since","")).strip()
    until=str(args.get("until","")).strip()
    limit=min(max(int(args.get("limit",20)),1),50)
    terms=[t for t in re.split(r"\W+",query) if t]
    matches=[]
    for line in audit_path.read_text(encoding="utf-8").splitlines():
        if not line.strip(): continue
        try:
            event=json.loads(line)
        except json.JSONDecodeError:
            continue
        if request_id and event.get("request_id") != request_id: continue
        if kind and event.get("kind") != kind: continue
        ts=str(event.get("timestamp",""))
        if since and ts < since: continue
        if until and ts > until: continue
        hay=" ".join(str(event.get(k,"")) for k in ("event_id","request_id","kind","actor","summary","result")).lower()
        if terms and not all(t in hay for t in terms): continue
        matches.append(event)
        if len(matches)>=limit: break
    return {"path":"state/AUDIT.jsonl","matches":matches,"count":len(matches),"truncated":len(matches)>=limit}

def memory_revision(args):
    memory_path=ROOT / "state" / "MEMORY.json"
    if not isinstance(args.get("entry"),dict):
        raise ValueError("memory_revision requires args.entry")
    entry=dict(args["entry"])
    required=("id","type","status","text","source","supersedes")
    if any(not str(entry.get(k,"")).strip() for k in required):
        raise ValueError("entry requires id,type,status,text,source,supersedes")
    if not re.fullmatch(r"[A-Za-z0-9._-]{1,80}",str(entry["id"])):
        raise ValueError("invalid memory entry id")
    memory=json.loads(memory_path.read_text(encoding="utf-8"))
    entries=memory.setdefault("entries",[])
    if any(e.get("id")==entry["id"] for e in entries):
        raise ValueError("revision id already exists")
    target=next((e for e in entries if e.get("id")==entry["supersedes"]),None)
    if target is None:
        raise ValueError("superseded entry not found")
    entry["type"]="revision"
    entries.append(entry)
    target["status"]="superseded"
    memory["updated"]=datetime.now(timezone.utc).date().isoformat()
    memory_path.write_text(json.dumps(memory,indent=2,ensure_ascii=False)+"\n",encoding="utf-8")
    return {"path":"state/MEMORY.json","action":"revised","superseded":entry["supersedes"],"entry":entry}

def memory_promote(args):
    memory_path=ROOT / "state" / "MEMORY.json"
    if not isinstance(args.get("entry"),dict):
        raise ValueError("memory_promote requires args.entry")
    entry=dict(args["entry"])
    required=("id","type","status","text","source")
    if any(not str(entry.get(k,"")).strip() for k in required):
        raise ValueError("entry requires id,type,status,text,source")
    if not re.fullmatch(r"[A-Za-z0-9._-]{1,80}",str(entry["id"])):
        raise ValueError("invalid memory entry id")
    if entry.get("status") not in ("active","verified","verified_pending"):
        raise ValueError("invalid memory entry status")
    memory=json.loads(memory_path.read_text(encoding="utf-8"))
    entries=memory.setdefault("entries",[])
    existing=next((e for e in entries if e.get("id")==entry["id"]),None)
    if existing is not None:
        if existing == entry:
            return {"path":"state/MEMORY.json","action":"unchanged","entry":entry}
        raise ValueError("memory entry id already exists with different content")
    entries.append(entry)
    memory["updated"]=datetime.now(timezone.utc).date().isoformat()
    memory_path.write_text(json.dumps(memory,indent=2,ensure_ascii=False)+"\n",encoding="utf-8")
    return {"path":"state/MEMORY.json","action":"added","entry":entry}

def memory_candidates(args):
    audit_path=ROOT / "state" / "AUDIT.jsonl"
    if not audit_path.exists():
        return {"source":"state/AUDIT.jsonl","candidates":[],"count":0}
    limit=min(max(int(args.get("limit",20)),1),50)
    candidates=[]
    seen=set()
    for line in audit_path.read_text(encoding="utf-8").splitlines():
        if not line.strip(): continue
        try: event=json.loads(line)
        except json.JSONDecodeError: continue
        if event.get("kind") not in ("checkpoint","result"): continue
        summary=str(event.get("summary","")).strip()
        result=str(event.get("result","")).strip()
        text=(summary+" "+result).strip()
        if not text: continue
        key=re.sub(r"\s+"," ",text.lower())
        if key in seen: continue
        seen.add(key)
        candidates.append({
            "candidate_id":"candidate-"+str(event.get("event_id","")),
            "type":"verified_candidate" if event.get("kind")=="checkpoint" else "result_candidate",
            "text":summary,
            "evidence":{"event_id":event.get("event_id"),"request_id":event.get("request_id"),"timestamp":event.get("timestamp")},
            "result":result
        })
        if len(candidates)>=limit: break
    return {"source":"state/AUDIT.jsonl","candidates":candidates,"count":len(candidates),"limit":limit}

def memory_boot(args):
    boot_path=ROOT / "state" / "BOOT.json"
    memory_path=ROOT / "state" / "MEMORY.json"
    boot=json.loads(boot_path.read_text(encoding="utf-8"))
    memory=json.loads(memory_path.read_text(encoding="utf-8"))
    query=str(args.get("query","")).strip().lower()
    entries=memory.get("entries",[])
    if query:
        terms=[t for t in re.split(r"\\W+",query) if t]
        scored=[]
        for e in entries:
            hay=(str(e.get("id",""))+" "+str(e.get("type",""))+" "+str(e.get("text",""))+" "+str(e.get("source",""))).lower()
            score=sum(1 for t in terms if t in hay)
            if score:
                scored.append((score,e))
        entries=[e for _,e in sorted(scored,key=lambda x:(-x[0],x[1].get("id","")))]
    limit=min(max(int(args.get("limit",20)),1),50)
    return {"boot":boot,"memory_schema":memory.get("schema"),"updated":memory.get("updated"),"entries":entries[:limit],"count":len(entries),"query":query}

HANDLERS={"inventory":inventory,"grep":grep,"sha256":sha256,"json_validate":json_validate,"python_compile":python_compile,"system_probe":system_probe,"read_text":read_text,"artifact_manifest":artifact_manifest,"diff_text":diff_text,"state_report":state_report,"cargo_check":cargo_check,"memory_boot":memory_boot,"audit_append":audit_append,"audit_search":audit_search,"memory_candidates":memory_candidates,"memory_promote":memory_promote,"memory_revision":memory_revision}

def process(path):
    with path.open(encoding="utf-8") as f: task=json.load(f)
    tid=task.get("id"); tool=task.get("tool")
    if not isinstance(tid,str) or not re.fullmatch(r"[A-Za-z0-9._-]{1,80}",tid): raise ValueError("invalid task id")
    if tool not in TOOLS: raise ValueError("tool not allowed")
    result={"schema":1,"task_id":tid,"tool":tool,"started":now()}
    try: result["ok"]=True; result["data"]=HANDLERS[tool](task.get("args") or {})
    except Exception as e: result["ok"]=False; result["error"]=f"{type(e).__name__}: {e}"
    result["finished"]=now(); RESULTS.mkdir(exist_ok=True)
    (RESULTS/f"{tid}.json").write_text(json.dumps(result,indent=2,ensure_ascii=False)+"\n",encoding="utf-8")
    path.unlink(); return result

def main():
    ap=argparse.ArgumentParser()
    ap.add_argument("--once",action="store_true")
    args=ap.parse_args()
    QUEUE.mkdir(exist_ok=True); RESULTS.mkdir(exist_ok=True)
    paths=sorted(QUEUE.glob("*.json"))
    if args.once:
        paths=paths[:1]
    processed=[]
    for p in paths:
        try: processed.append(process(p))
        except Exception as e:
            tid=p.stem; (RESULTS/f"{tid}.json").write_text(json.dumps({"schema":1,"task_id":tid,"ok":False,"error":f"{type(e).__name__}: {e}","finished":now()},indent=2)+"\n"); p.unlink(); processed.append({"task_id":tid,"ok":False})
    state=json.loads(STATE.read_text()) if STATE.exists() else {}
    for r in processed:
        state["completed"]=int(state.get("completed",0))+(1 if r.get("ok") else 0)
        state["failed"]=int(state.get("failed",0))+(0 if r.get("ok") else 1)
        state["last_task"]=r.get("task_id"); state["last_result"]=f"results/{r.get('task_id')}.json"
    state["last_worker"]=now(); state["status"]="pending" if list(QUEUE.glob("*.json")) else "idle"
    STATE.parent.mkdir(exist_ok=True); STATE.write_text(json.dumps(state,indent=2)+"\n")
    print(json.dumps({"processed":len(processed),"state":state},indent=2))

if __name__=="__main__": main()
