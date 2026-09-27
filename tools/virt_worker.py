#!/usr/bin/env python3
"""Small deterministic worker for the Virt Extension Layer."""
from __future__ import annotations
import argparse, hashlib, json, os, platform, re, sys
from datetime import datetime, timezone
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
QUEUE = ROOT / "queue"
RESULTS = ROOT / "results"
STATE = ROOT / "state" / "index.json"
TOOLS = {"inventory", "grep", "sha256", "json_validate", "python_compile", "system_probe"}

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
        h=hashlib.sha256();
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

HANDLERS={"inventory":inventory,"grep":grep,"sha256":sha256,"json_validate":json_validate,"python_compile":python_compile,"system_probe":system_probe}

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
    ap=argparse.ArgumentParser(); ap.add_argument("--once",action="store_true"); ap.parse_args()
    QUEUE.mkdir(exist_ok=True); RESULTS.mkdir(exist_ok=True)
    paths=sorted(QUEUE.glob("*.json")); processed=[]
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
