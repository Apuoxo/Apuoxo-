//! Aether Text Editor
use crate::fs;
use crate::graphics;
pub const CAP: usize = 16384;
pub const NAME_MAX: usize = 28;
const LINE_H: usize = 16;
const TOOL_H: usize = 30;
const STATUS_H: usize = 22;
static mut BUF: [u8; CAP] = [0; CAP];
static mut LEN: usize = 0;
static mut CUR: usize = 0;
static mut NAME: [u8; NAME_MAX] = [0; NAME_MAX];
static mut NAME_LEN: usize = 0;
static mut DIRTY: bool = false;
static mut MODE: u8 = 0;
static mut DIALOG: [u8; NAME_MAX] = [0; NAME_MAX];
static mut DIALOG_LEN: usize = 0;
static mut SCROLL: usize = 0;
static mut STATUS: [u8; 64] = [0; 64];
static mut STATUS_LEN: usize = 0;
static mut BLINK: u32 = 0;
static mut PICK_N: usize = 0;
static mut PICK_SEL: usize = 0;
static mut PICK_NAME: [[u8; 24]; 16] = [[0; 24]; 16];
static mut PICK_LEN: [usize; 16] = [0; 16];
fn set_status(msg: &[u8]) {
    unsafe {
        let mut i = 0usize;
        while i < 64 { STATUS[i] = 0; i += 1; }
        i = 0;
        while i < msg.len() && i < 63 { STATUS[i] = msg[i]; i += 1; }
        STATUS_LEN = i;
    }
}
fn name_str() -> &'static str {
    unsafe {
        let n = if NAME_LEN > NAME_MAX { NAME_MAX } else { NAME_LEN };
        core::str::from_utf8(&NAME[..n]).unwrap_or("UNTITLED.TXT")
    }
}
fn is_untitled() -> bool {
    unsafe { NAME_LEN >= 8 && NAME[0]==b'U' && NAME[1]==b'N' && NAME[2]==b'T' }
}
fn normalize_name(raw: &[u8], out: &mut [u8; NAME_MAX]) -> usize {
    let mut n = 0usize; let mut i = 0usize;
    while i < raw.len() && n < NAME_MAX - 1 {
        let mut c = raw[i];
        if c >= b'a' && c <= b'z' { c -= 32; }
        let ok = (c>=b'A'&&c<=b'Z')||(c>=b'0'&&c<=b'9')||c==b'.'||c==b'_'||c==b'-';
        if ok { out[n] = c; n += 1; }
        i += 1;
    }
    let mut has_dot = false; let mut j = 0usize;
    while j < n { if out[j]==b'.' { has_dot = true; } j += 1; }
    if n > 0 && !has_dot && n + 4 < NAME_MAX {
        out[n]=b'.'; out[n+1]=b'T'; out[n+2]=b'X'; out[n+3]=b'T'; n += 4;
    }
    n
}
fn line_of(off: usize) -> usize {
    unsafe {
        let mut line=0usize; let mut i=0usize;
        let lim=if off>LEN{LEN}else{off};
        while i<lim { if BUF[i]==b'\n'{line+=1;} i+=1; }
        line
    }
}
fn line_start(line: usize) -> usize {
    unsafe {
        if line==0{return 0;}
        let mut l=0usize; let mut i=0usize;
        while i<LEN {
            if BUF[i]==b'\n'{l+=1; if l==line{return i+1;}}
            i+=1;
        }
        LEN
    }
}
fn col_of(off: usize) -> usize {
    unsafe {
        let mut col=0usize; let mut i=off;
        while i>0 { i-=1; if BUF[i]==b'\n'{break;} col+=1; }
        col
    }
}
fn ensure_cursor_visible(rows: usize) {
    unsafe {
        let line=line_of(CUR);
        if line<SCROLL{SCROLL=line;}
        else if rows>0&&line>=SCROLL+rows{SCROLL=line+1-rows;}
    }
}
pub fn new_file() {
    unsafe {
        LEN=0; CUR=0; DIRTY=false; MODE=0; SCROLL=0; DIALOG_LEN=0;
        let s=b"UNTITLED.TXT";
        let mut i=0usize; while i<NAME_MAX{NAME[i]=0;i+=1;}
        i=0; while i<s.len(){NAME[i]=s[i];i+=1;}
        NAME_LEN=s.len();
        set_status(b"New - Save As to write disk");
    }
}
pub fn open_bytes(name: &[u8], data: &[u8]) {
    unsafe {
        let mut norm=[0u8;NAME_MAX];
        let nl=normalize_name(name,&mut norm);
        let mut i=0usize; while i<NAME_MAX{NAME[i]=0;i+=1;}
        i=0; while i<nl{NAME[i]=norm[i];i+=1;}
        NAME_LEN=nl;
        i=0; while i<data.len()&&i<CAP{BUF[i]=data[i];i+=1;}
        LEN=i; CUR=0; DIRTY=false; MODE=0; SCROLL=0;
        set_status(b"Opened");
    }
}
pub fn open_named(name: &[u8]) -> bool {
    if !fs::is_mounted(){set_status(b"AetherFS not mounted");return false;}
    let mut norm=[0u8;NAME_MAX];
    let nl=normalize_name(name,&mut norm);
    if nl==0{set_status(b"Bad name");return false;}
    let path=core::str::from_utf8(&norm[..nl]).unwrap_or("");
    let mut tmp=[0u8;CAP];
    let n=fs::read(path,&mut tmp).or_else(||{
        let mut p=[0u8;32]; p[0]=b'/';
        let mut i=0usize; while i<nl&&i+1<32{p[i+1]=norm[i];i+=1;}
        fs::read(core::str::from_utf8(&p[..i+1]).unwrap_or(path),&mut tmp)
    });
    let Some(n)=n else{set_status(b"File not found");return false;};
    open_bytes(&norm[..nl],&tmp[..n]);
    true
}
pub fn save() -> bool {
    unsafe {
        if NAME_LEN==0||is_untitled(){
            begin_save_as(); set_status(b"Type name + Enter"); return false;
        }
        if !fs::is_mounted(){set_status(b"AetherFS not mounted");return false;}
        let path=name_str();
        let _=fs::create(path);
        let data=&BUF[..LEN];
        let ok=if data.is_empty(){fs::write(path,b"")}
            else if data.len()<=512{fs::write(path,data)||fs::write_large(path,data)}
            else{fs::write_large(path,data)};
        if ok{DIRTY=false;set_status(b"Saved OK to AetherFS");true}
        else{set_status(b"SAVE FAILED");false}
    }
}
pub fn begin_save_as(){unsafe{MODE=1;DIALOG_LEN=0;set_status(b"Save As: name+Enter Esc=cancel");}}
fn begin_open_picker(){
    unsafe{
        MODE=3;PICK_N=0;PICK_SEL=0;
        if !fs::is_mounted(){MODE=0;set_status(b"AetherFS not mounted");return;}
        let mut items=[fs::ListItem{name:[0;24],name_len:0,size:0,is_dir:false};16];
        let n=fs::list_ex(&mut items);
        let mut i=0usize; let mut k=0usize;
        while i<n&&k<16{
            if !items[i].is_dir&&items[i].name_len>0{
                let mut j=0usize; while j<24{PICK_NAME[k][j]=0;j+=1;}
                j=0; while j<items[i].name_len&&j<24{PICK_NAME[k][j]=items[i].name[j];j+=1;}
                PICK_LEN[k]=items[i].name_len; k+=1;
            }
            i+=1;
        }
        PICK_N=k;
        if k==0{MODE=2;set_status(b"Type filename + Enter");}
        else{set_status(b"Up/Down Enter open");}
    }
}
pub fn cancel_mode(){unsafe{MODE=0;set_status(b"Edit");}}
pub fn is_dialog()->bool{unsafe{MODE!=0}}
fn insert_byte(ch:u8){unsafe{
    if LEN>=CAP-1{set_status(b"Buffer full");return;}
    let mut i=LEN; while i>CUR{BUF[i]=BUF[i-1];i-=1;}
    BUF[CUR]=ch;LEN+=1;CUR+=1;DIRTY=true;
}}
fn delete_back(){unsafe{
    if CUR==0{return;}
    let mut i=CUR-1; while i+1<LEN{BUF[i]=BUF[i+1];i+=1;}
    LEN-=1;CUR-=1;DIRTY=true;
}}
fn dialog_char(ch:u8){unsafe{
    if ch==b'\n'{
        if MODE==1{
            if DIALOG_LEN==0{set_status(b"Empty name");return;}
            let mut norm=[0u8;NAME_MAX];
            let nl=normalize_name(&DIALOG[..DIALOG_LEN],&mut norm);
            let mut i=0usize; while i<NAME_MAX{NAME[i]=0;i+=1;}
            i=0; while i<nl{NAME[i]=norm[i];i+=1;}
            NAME_LEN=nl; MODE=0; let _=save();
        } else if MODE==2{
            if DIALOG_LEN==0{return;}
            let mut n=[0u8;NAME_MAX]; let mut i=0usize;
            while i<DIALOG_LEN&&i<NAME_MAX{n[i]=DIALOG[i];i+=1;}
            MODE=0; let _=open_named(&n[..i]);
        } else if MODE==3{
            if PICK_N>0&&PICK_SEL<PICK_N{
                let nl=PICK_LEN[PICK_SEL]; MODE=0;
                let _=open_named(&PICK_NAME[PICK_SEL][..nl]);
            }
        }
        return;
    }
    if ch==0x08{if MODE!=3&&DIALOG_LEN>0{DIALOG_LEN-=1;}return;}
    if ch==0x1B{MODE=0;set_status(b"Cancelled");return;}
    if MODE==3{return;}
    let ok=(ch>=b'A'&&ch<=b'Z')||(ch>=b'a'&&ch<=b'z')||(ch>=b'0'&&ch<=b'9')||ch==b'.'||ch==b'_'||ch==b'-';
    if ok&&DIALOG_LEN<NAME_MAX-1{DIALOG[DIALOG_LEN]=ch;DIALOG_LEN+=1;}
}}
pub fn on_char(ch:u8){unsafe{
    if MODE!=0{dialog_char(ch);return;}
    if ch==0x08{delete_back();return;}
    if ch==b'\n'||(ch>=32&&ch<127){insert_byte(ch);}
}}
pub fn on_special(hid:u8)->bool{unsafe{
    if MODE==3{
        return match hid{
            0x52=>{if PICK_SEL>0{PICK_SEL-=1;}true}
            0x51=>{if PICK_SEL+1<PICK_N{PICK_SEL+=1;}true}
            0x28=>{dialog_char(b'\n');true}
            0x29=>{cancel_mode();true}
            _=>false
        };
    }
    if MODE!=0{return false;}
    match hid{
        0x50=>{if CUR>0{CUR-=1;}true}
        0x4F=>{if CUR<LEN{CUR+=1;}true}
        0x52=>{
            let col=col_of(CUR); let line=line_of(CUR);
            if line>0{
                let start=line_start(line-1);
                let mut plen=0usize; let mut j=start;
                while j<LEN&&BUF[j]!=b'\n'{plen+=1;j+=1;}
                CUR=start+if col<plen{col}else{plen};
            }
            true
        }
        0x51=>{
            let col=col_of(CUR); let line=line_of(CUR);
            let next=line_start(line+1);
            if next<=LEN{
                let mut plen=0usize; let mut j=next;
                while j<LEN&&BUF[j]!=b'\n'{plen+=1;j+=1;}
                CUR=next+if col<plen{col}else{plen};
                if CUR>LEN{CUR=LEN;}
            }
            true
        }
        0x4A=>{CUR=line_start(line_of(CUR));true}
        0x4D=>{
            let start=line_start(line_of(CUR));
            let mut j=start; while j<LEN&&BUF[j]!=b'\n'{j+=1;}
            CUR=j; true
        }
        0x4B=>{SCROLL=SCROLL.saturating_sub(10);true}
        0x4E=>{SCROLL+=10;true}
        0x2A=>{delete_back();true}
        _=>false
    }
}}
pub fn toolbar_click(btn:usize){
    match btn{
        0=>new_file(),
        1=>begin_open_picker(),
        2=>{let _=save();},
        3=>begin_save_as(),
        _=>{}
    }
}
pub fn click(mx:i32,my:i32,wx:i32,wy:i32,title_h:i32)->bool{
    if my>=wy+title_h && my<wy+title_h+TOOL_H as i32{
        let rel=mx-wx-8;
        if rel>=0{
            let btn=(rel/64)as usize;
            if btn<4{toolbar_click(btn);return true;}
        }
    }
    false
}
pub fn paint(wx:usize,wy:usize,ww:usize,wh:usize,title_h:usize){
    unsafe{
        BLINK=BLINK.wrapping_add(1);
        let th=title_h;
        if ww<100||wh<th+TOOL_H+STATUS_H+30{return;}
        graphics::fill_rect(wx+2,wy+th,ww-4,TOOL_H,0x00E4EAF0);
        let labels=["New","Open","Save","SaveAs"];
        let mut bi=0usize;
        while bi<4{
            let bx=wx+8+bi*64;
            graphics::fill_rect(bx,wy+th+5,60,20,0x00C8D4E0);
            graphics::border_rect(bx,wy+th+5,60,20,0x00405060);
            graphics::draw_str(bx+10,wy+th+9,labels[bi],0x00101820);
            bi+=1;
        }
        let mut nb=[0u8;40]; let mut ni=0usize;
        while ni<NAME_LEN&&ni<28{nb[ni]=NAME[ni];ni+=1;}
        if DIRTY&&ni<39{nb[ni]=b'*';ni+=1;}
        if let Ok(s)=core::str::from_utf8(&nb[..ni]){
            graphics::draw_str(wx+280,wy+th+9,s,0x00202830);
        }
        let ty=wy+th+TOOL_H;
        let text_h=wh.saturating_sub(th+TOOL_H+STATUS_H);
        graphics::fill_rect(wx+2,ty,ww-4,text_h,0x00FFFFF8);
        graphics::border_rect(wx+2,ty,ww-4,text_h,0x00A0A8B0);
        let cols=((ww-20)/8).max(8);
        let rows=(text_h/LINE_H).max(1);
        ensure_cursor_visible(rows);
        let mut line=0usize; let mut i=0usize;
        while line<SCROLL&&i<LEN{if BUF[i]==b'\n'{line+=1;}i+=1;}
        let mut row=0usize; let mut col=0usize;
        while i<=LEN&&row<rows{
            if i==CUR&&MODE==0&&(BLINK/12)%2==0{
                graphics::fill_rect(wx+8+col*8,ty+3+row*LINE_H,2,14,0x000050C0);
            }
            if i>=LEN{break;}
            let ch=BUF[i];
            if ch==b'\n'{col=0;row+=1;i+=1;continue;}
            if col>=cols{col=0;row+=1;if row>=rows{break;}}
            let dc=if ch>=32&&ch<127{ch}else{b'?'};
            graphics::draw_char(wx+8+col*8,ty+3+row*LINE_H,dc,0x00101010);
            col+=1;i+=1;
        }
        let sy=wy+wh.saturating_sub(STATUS_H);
        graphics::fill_rect(wx+2,sy,ww-4,STATUS_H-2,0x00E0E6EC);
        if STATUS_LEN>0{
            if let Ok(s)=core::str::from_utf8(&STATUS[..STATUS_LEN]){
                graphics::draw_str(wx+8,sy+5,s,0x00202830);
            }
        }
        if MODE==1||MODE==2{
            let title=if MODE==1{"Save As - enter filename"}else{"Open - enter filename"};
            let mw=340usize; let mh=90usize;
            let mx=wx+ww.saturating_sub(mw)/2; let my=wy+wh/2-45;
            graphics::fill_rect(mx+3,my+3,mw,mh,0x00404040);
            graphics::fill_rect(mx,my,mw,mh,0x00F8F8FF);
            graphics::border_rect(mx,my,mw,mh,0x002060A0);
            graphics::fill_rect(mx,my,mw,22,0x002060A0);
            graphics::draw_str(mx+10,my+5,title,0x00FFFFFF);
            graphics::draw_str(mx+12,my+30,"Name + Enter | Esc cancel",0x00404040);
            graphics::fill_rect(mx+12,my+52,mw-24,22,0x00FFFFFF);
            graphics::border_rect(mx+12,my+52,mw-24,22,0x008090A0);
            let mut si=0usize;
            while si<DIALOG_LEN{
                graphics::draw_char(mx+16+si*8,my+56,DIALOG[si],0x00101010);
                si+=1;
            }
        }
        if MODE==3{
            let mw=280usize; let mh=220usize; let mx=wx+40; let my=wy+60;
            graphics::fill_rect(mx+3,my+3,mw,mh,0x00404040);
            graphics::fill_rect(mx,my,mw,mh,0x00F8F8FF);
            graphics::border_rect(mx,my,mw,mh,0x002060A0);
            graphics::fill_rect(mx,my,mw,24,0x002060A0);
            graphics::draw_str(mx+10,my+6,"Open file (AetherFS)",0x00FFFFFF);
            let mut r=0usize;
            while r<PICK_N{
                let yy=my+30+r*16;
                if r==PICK_SEL{graphics::fill_rect(mx+4,yy,mw-8,16,0x00C0D8F0);}
                if let Ok(s)=core::str::from_utf8(&PICK_NAME[r][..PICK_LEN[r]]){
                    graphics::draw_str(mx+10,yy+2,s,0x00101010);
                }
                r+=1;
            }
        }
    }
}
pub fn is_text_name(name:&[u8])->bool{
    let n=name.len(); if n<3{return false;}
    let mut dot=n; let mut i=n;
    while i>0{i-=1; if name[i]==b'.'{dot=i;break;}}
    if dot>=n-1{return false;}
    let ext=&name[dot+1..];
    let eq=|a:&[u8],b:&[u8]|{
        if a.len()!=b.len(){return false;}
        let mut i=0usize;
        while i<a.len(){
            let ca=if a[i]>=b'a'&&a[i]<=b'z'{a[i]-32}else{a[i]};
            if ca!=b[i]{return false;}
            i+=1;
        }
        true
    };
    eq(ext,b"TXT")||eq(ext,b"LOG")||eq(ext,b"MD")||eq(ext,b"CFG")||eq(ext,b"INI")||eq(ext,b"CSV")
}
