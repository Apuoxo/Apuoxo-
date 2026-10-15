//! Aether Explorer - native Windows 7 inspired file manager.
//! This replaces the old XP-style file window.  It is an original Aether UI,
//! not a copy of Microsoft code/assets.

use crate::fs;
use crate::graphics;
use crate::gui::{font, icon, theme};
use crate::serial;
use crate::part;
use crate::{fs_fat, fs_ntfs};

const WHITE:u32=0x00FFFFFF;
const TEXT:u32=0x001A1A1A;      // near-black, Win7 body
const DIM:u32=0x005A5A5A;       // secondary labels
const BORDER:u32=0x00A8B0BC;
const TOOL:u32=0x00F7FAFC;
const TOOL2:u32=0x00E4ECF5;
const GLASS:u32=0x00EEF5FC;
const BLUE:u32=0x002B6CB0;      // Win7 accent
const BLUE2:u32=0x00CCE8FF;     // selection fill
const SELECT:u32=0x00CCE8FF;
const SIDE:u32=0x00F0F5FA;
const GREEN:u32=0x003B8D3B;
const RED:u32=0x00C42B1C;

const W7_BAR_T:u32=0x00FAFBFD; const W7_BAR_B:u32=0x00E5EAF3;
const W7_HDR_T:u32=0x00FFFFFF; const W7_HDR_B:u32=0x00EEF2F7;
const W7_SEP:u32=0x00D0D0D0;
const W7_NAV:u32=0x001E395B;
const W7_HOVER:u32=0x00E5F3FF;

fn lerp(a:u32,b:u32,t:usize,n:usize)->u32{
    let mut o=0u32;let mut s=0;
    while s<=16{
        let ca=((a>>s)&0xFF) as i32;let cb=((b>>s)&0xFF) as i32;
        o|=((ca+(cb-ca)*(t as i32)/(n as i32)) as u32)<<s;s+=8;
    }
    o
}
fn vgrad(x:usize,y:usize,w:usize,h:usize,top:u32,bot:u32){
    let mut i=0;while i<h{graphics::fill_rect(x,y+i,w,1,lerp(top,bot,i,h.max(1)));i+=1;}
}
fn sel_row(x:usize,y:usize,w:usize,h:usize,hot:bool){
    // Win7 list selection: soft blue fill + thin border
    let (t,b,br)=if hot{(0x00F2F9FF,0x00E5F3FF,0x00A8D0F0)}else{(0x00E8F4FC,0x00CCE8FF,0x0026A0DA)};
    vgrad(x+1,y+1,w.saturating_sub(2),h.saturating_sub(2),t,b);
    graphics::border_rect(x,y,w,h,br);
}
/// Soft title-case for ALL-CAPS 8.3 names (display only).
fn draw_name_soft(x:usize,y:usize,name:&[u8],len:usize,maxc:usize,col:u32){
    let mut all_up=true; let mut i=0usize;
    while i<len{ let c=name[i]; if c>=b'a'&&c<=b'z'{all_up=false;break;} i+=1; }
    let mut prev_sep=true; let mut k=0usize; let mut cx=x;
    while k<len && k<maxc{
        let mut c=name[k];
        if all_up && c>=b'A'&&c<=b'Z'{
            if !prev_sep { c = c + 32; } // lower after first of word
        }
        graphics::draw_char(cx,y,c,col);
        prev_sep = c==b' '||c==b'.'||c==b'_'||c==b'-';
        cx+=8; k+=1;
    }
}

pub const VIEW_COMPUTER:u8=0;
pub const VIEW_ROOT:u8=1;
pub const VIEW_TEXT:u8=3;
pub const VIEW_DISK:u8=4;
pub const VIEW_FAT:u8=5;
pub const VIEW_NTFS:u8=6;

static mut VIEW:u8=VIEW_COMPUTER;
static mut SEL:i32=-1;
static mut HOVER:i32=-1;
static mut COMPUTER_SEL:i32=-1;
static mut FOCUS_ADDR:bool=false;
static mut CTX:bool=false;
static mut CTX_X:i32=0;
static mut CTX_Y:i32=0;
static mut CONFIRM_DEL:bool=false;
/// Clipboard: 0=empty 1=copy 2=cut (move)
static mut CLIP_MODE:u8=0;
static mut CLIP_IS_DIR:bool=false;
static mut CLIP_NAME:[u8;64]=[0;64];
static mut CLIP_NAME_LEN:usize=0;
static mut CLIP_PATH:[u8;96]=[0;96];
static mut CLIP_PATH_LEN:usize=0;
static mut CLIP_DATA:[u8;65536]=[0;65536];
static mut CLIP_DATA_LEN:usize=0;
static mut CLIP_SRC:u8=0; // 0=none 1=aether 2=ntfs 3=fat
static mut CLIP_MFT:u32=0;
static mut CLIP_FAT_IDX:u32=0;
static mut HIST:[[u8;32];8]=[[0;32];8];
static mut HIST_LEN:[usize;8]=[0;8];
static mut HIST_N:usize=0;
static mut HIST_I:usize=0;
static mut CWD:[u8;32]=[b'/',0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0];
static mut CWD_LEN:usize=1;
static mut NTFS_CWD_REF:u32=5;
static mut NTFS_PARENT:[u32;8]=[0;8];
static mut NTFS_SEG:[[u8;28];8]=[[0;28];8];
static mut NTFS_SEG_LEN:[usize;8]=[0;8];
static mut NTFS_DEPTH:usize=0;
static mut NTFS_VIEW_IDX:[u8;256]=[0;256];
static mut NTFS_VIEW_COUNT:usize=0;
static mut NTFS_SCROLL:usize=0;
static mut NTFS_VIEW_DIRTY:bool=true;
static mut SORT_MODE:u8=0; // 0 name 1 size 2 date
static mut NTFS_VISIBLE_ROWS:usize=1;
static mut PREVIEW:[u8;2048]=[0;2048];
static mut PREVIEW_LEN:usize=0;
/// View to restore when leaving text preview (VIEW_NTFS/FAT/ROOT).
static mut PREVIEW_FROM:u8=VIEW_COMPUTER;
static mut STATUS:[u8;64]=[0;64];
static mut STATUS_LEN:usize=0;
static mut LAST_SC:u8=0;
static mut LAST_NAV_TAG:u8=0; // 1=UP 2=DN
/// Record raw scancode for status bar (survives redraw).
pub fn note_scancode(sc:u8){unsafe{LAST_SC=sc;}}
/// 0 = none, 1 = selection-only (no full window wipe), 2 = full content redraw
static mut PAINT_HINT:u8=0;
static mut PAINT_OLD_SEL:i32=-1;

/// After on_click/on_nav_key: 0 none, 1 selection rows only, 2 full explorer content.
pub fn take_paint_hint()->u8{unsafe{let h=PAINT_HINT;PAINT_HINT=0;h}}
// Partial row paint caused AH532 reboots - always full content redraw.
fn hint_sel(_old:i32){unsafe{PAINT_HINT=2;}}
fn hint_full(){unsafe{PAINT_HINT=2;}}

/// Repaint only old+new list rows (no full window). Used for selection changes.
pub fn paint_selection_rows(wx:usize,wy:usize,ww:usize,wh:usize,title_h:usize){
    unsafe{
        let sw=graphics::width();
        let sh_fb=graphics::height();
        if ww<32||wh<title_h+40||wx>=sw||wy>=sh_fb{return;}
        let by=wy+title_h;
        let sy=by+56;
        let side=160usize.min(ww/3);
        let cx=wx+side+8;
        let cw=ww.saturating_sub(side+16);
        if cx+32>=sw||cw<80{return;}
        if VIEW!=VIEW_NTFS{
            // Non-NTFS: still update status strip only
            paint_status_strip(wx,wy,ww,wh,sw,sh_fb);
            return;
        }
        if NTFS_VIEW_DIRTY{rebuild_ntfs_view();}
        let sh=wh.saturating_sub(title_h+78);
        let rows=visible_rows(sh);
        NTFS_VISIBLE_ROWS=if rows==0{1}else{rows};
        let old=PAINT_OLD_SEL;
        let new=SEL;
        let paint_row=|r:i32|{
            if r<0{return;}
            let ru=r as usize;
            if ru>=NTFS_VISIBLE_ROWS{return;}
            let ry=sy+ROW_TOP as usize+ru*32;
            if ry+ROW_H as usize>=sh_fb{return;}
            let selected=SEL==r;
            let hovered=HOVER==r;
            // clear row + selection/hover chrome
            graphics::fill_rect(cx,ry,cw.min(sw.saturating_sub(cx)),ROW_H as usize,WHITE);
            if selected{sel_row(cx+2,ry,cw.saturating_sub(4),ROW_H as usize,false);}
            else if hovered{sel_row(cx+2,ry,cw.saturating_sub(4),ROW_H as usize,true);}
            let vi=NTFS_SCROLL+ru;
            let idx=match ntfs_row_index(vi){Some(v)=>v,None=>return};
            if let Some(e)=fs_ntfs::entry(idx){
                if e.is_dir{icon::blit(icon::IconId::Folder,cx+7,ry,false);}
                else{icon::blit(icon::IconId::File,cx+7,ry,false);}
                let max_chars=((cw.saturating_sub(50))/8).min(36).max(8);
                let mut k=0;while k<e.name_len&&k<max_chars{
                    graphics::draw_char(cx+42+k*8,ry+10,e.name[k],TEXT);k+=1;
                }
                if cw>400{draw_mtime(cx+250,ry+10,e.mtime);}
                if cw>520{graphics::draw_str(cx+400,ry+10,file_type_label(&e.name,e.name_len,e.is_dir),DIM);}
                if cw>600&&!e.is_dir{
                    let sz=match fs_ntfs::file_data_size(e.mft_ref){Some(n) if n>0=>n as u64,_=>e.size};
                    draw_size_kb(cx+520,ry+10,sz);
                }
            }
        };
        if old!=new{paint_row(old);}
        paint_row(new);
        paint_status_strip(wx,wy,ww,wh,sw,sh_fb);
    }
}

fn paint_status_strip(wx:usize,wy:usize,ww:usize,wh:usize,sw:usize,sh_fb:usize){
    unsafe{
        if wh<=22||wy+wh>sh_fb{return;}
        let status_y=wy+wh-22;
        vgrad(wx+3,status_y,(ww-6).min(sw.saturating_sub(wx+3)),20,0x00F8FAFC,0x00E4EAF2);
        graphics::border_rect(wx+3,status_y,(ww-6).min(sw.saturating_sub(wx+3)),20,0x00A8B0BC);
        // rebuild status from selection
        fill_status_bar();
        let mut i=0;while i<STATUS_LEN&&i<((ww.saturating_sub(20))/8){
            graphics::draw_char(wx+10+i*8,status_y+6,STATUS[i],TEXT);i+=1;
        }
    }
}

/// Build STATUS - always includes SC= and SEL= so AH532 diagnosis survives redraw.
fn fill_status_bar(){
    unsafe{
        let mut b=[0u8;64]; let mut p=0usize;
        // Nav tag
        if LAST_NAV_TAG==1{ put_str(&mut b,&mut p,b"UP "); }
        else if LAST_NAV_TAG==2{ put_str(&mut b,&mut p,b"DN "); }
        // Raw scancode (never cleared by redraw)
        put_str(&mut b,&mut p,b"SC=");
        let hi=LAST_SC>>4;
        let lo=LAST_SC&0xF;
        if p<62{b[p]=if hi<10{b'0'+hi}else{b'A'+(hi-10)}; p+=1;}
        if p<62{b[p]=if lo<10{b'0'+lo}else{b'A'+(lo-10)}; p+=1;}
        put_str(&mut b,&mut p,b" SEL=");
        if SEL<0{ put_str(&mut b,&mut p,b"-"); }
        else{ put_num(&mut b,&mut p,SEL as usize); }
        if VIEW==VIEW_NTFS{
            put_str(&mut b,&mut p,b" N=");
            put_num(&mut b,&mut p,NTFS_VIEW_COUNT);
            if SEL>=0 && !NTFS_VIEW_DIRTY{
                let abs=NTFS_SCROLL+(SEL as usize);
                if abs<NTFS_VIEW_COUNT{
                    if let Some(idx)=ntfs_row_index(abs){
                        if let Some(e)=fs_ntfs::entry(idx){
                            put_str(&mut b,&mut p,b" ");
                            let mut k=0;while k<e.name_len&&k<12&&p<60{b[p]=e.name[k];p+=1;k+=1;}
                        }
                    }
                }
            }
        }else if VIEW==VIEW_ROOT{
            put_str(&mut b,&mut p,b" AetherFS");
        }else if VIEW==VIEW_COMPUTER{
            put_str(&mut b,&mut p,b" Computer");
        }else if VIEW==VIEW_DISK{
            put_str(&mut b,&mut p,b" Disk");
        }
        let mut i=0;while i<64{STATUS[i]=0;i+=1;}
        i=0;while i<p&&i<63{STATUS[i]=b[i];i+=1;}
        STATUS_LEN=i;
    }
}

#[derive(Clone,Copy)]
struct Props{
    kind:u8, // 1=file, 2=folder
    name:[u8;64],
    name_len:usize,
    size:u64,
    alloc:u64,
    created:u64,
    modified:u64,
    accessed:u64,
    attrs:u32,
    files:u32,
    dirs:u32,
    mft:u32,
}
static mut PROPS_OPEN:bool=false;
static mut PROPS:Props=Props{
    kind:0,name:[0;64],name_len:0,size:0,alloc:0,
    created:0,modified:0,accessed:0,attrs:0,files:0,dirs:0,mft:0
};

struct Buf{b:[u8;72],n:usize}
impl Buf{
    fn new()->Self{Buf{b:[0;72],n:0}}
    fn s(&mut self,t:&str){for c in t.bytes(){if self.n<72{self.b[self.n]=c;self.n+=1;}}}
    fn u(&mut self,mut v:u64){let mut d=[0u8;20];let mut c=0;if v==0{d[0]=b'0';c=1;}while v>0{d[c]=(v%10) as u8+b'0';v/=10;c+=1;}while c>0{c-=1;if self.n<72{self.b[self.n]=d[c];self.n+=1;}}}
    fn size(&mut self,v:u64){
        let (dv,u)=if v>=1<<30{(1u64<<30,"GB")}else if v>=1<<20{(1u64<<20,"MB")}else if v>=1<<10{(1u64<<10,"KB")}else{(1,"B")};
        self.u(v/dv);if dv>1{self.s(".");self.u((v%dv)*10/dv);}self.s(" ");self.s(u);
    }
    fn as_str(&self)->&str{unsafe{core::str::from_utf8_unchecked(&self.b[..self.n])}}
}
fn prow(x:usize,y:&mut usize,label:&str,v:&Buf){
    graphics::draw_str(x,*y,label,DIM);graphics::draw_str(x+110,*y,v.as_str(),TEXT);*y+=18;
}
fn prop_name(){
    unsafe{
        let mut i=0;
        while i<48{PROPS.name[i]=0;i+=1;}
    }
}
fn fill_props_selected(){
    unsafe{
        prop_name();
        PROPS.kind=0;PROPS.name_len=0;PROPS.size=0;PROPS.alloc=0;
        PROPS.created=0;PROPS.modified=0;PROPS.accessed=0;PROPS.attrs=0;
        PROPS.files=0;PROPS.dirs=0;PROPS.mft=0;
        if VIEW==VIEW_NTFS{
            if NTFS_VIEW_DIRTY{rebuild_ntfs_view();}
            if SEL<0{return;}
            let idx=match ntfs_row_index(NTFS_SCROLL+SEL as usize){Some(v)=>v,None=>return};
            if let Some(e)=fs_ntfs::entry(idx){
                PROPS.kind=if e.is_dir{2}else{1};
                PROPS.name_len=e.name_len.min(48);
                let mut i=0;while i<PROPS.name_len{PROPS.name[i]=e.name[i];i+=1;}
                let sz=match fs_ntfs::file_data_size(e.mft_ref){Some(n) if n>0=>n as u64,_=>e.size};
                PROPS.size=sz;PROPS.alloc=sz;PROPS.modified=e.mtime;PROPS.mft=e.mft_ref;
                PROPS_OPEN=true;
                status(b"Properties opened");
            }
        }else if VIEW==VIEW_ROOT{
            if SEL<0{return;}
            let mut a=[fs::ListItem{name:[0;24],name_len:0,size:0,is_dir:false};16];
            let n=fs::list_ex_path(core::str::from_utf8_unchecked(&CWD[..CWD_LEN]),&mut a);
            let s=SEL as usize;
            if s>=n{return;}
            PROPS.kind=if a[s].is_dir{2}else{1};
            PROPS.name_len=a[s].name_len.min(48);
            let mut i=0;while i<PROPS.name_len{PROPS.name[i]=a[s].name[i];i+=1;}
            PROPS.size=a[s].size as u64;PROPS.alloc=PROPS.size;
            PROPS_OPEN=true;
            status(b"Properties opened");
        }
    }
}
/// Context menu row height and geometry (Win7-style popup).
const CTX_W:i32=236;
const CTX_ROW:i32=22;
/// Item index from mouse Y relative to menu top. Separators return None.
fn context_item(rel:i32)->Option<usize>{
    // rows: 0 Open, 1 Open with, sep, 2 Copy, 3 Cut, 4 Paste, sep,
    // 5 Rename, 6 Delete, sep, 7 Properties, 8 Refresh, sep, 9 New folder, 10 New file
    let rows:[i32;11]=[0,1,3,4,5,7,8,10,11,13,14];
    let mut i=0usize;
    while i<11{
        let top=4+rows[i]*CTX_ROW;
        if rel>=top&&rel<top+CTX_ROW{return Some(i);}
        i+=1;
    }
    None
}
fn fs_writable()->bool{
    unsafe{
        (VIEW==VIEW_ROOT&&crate::fs::is_mounted())
        || (VIEW==VIEW_FAT&&crate::fs_fat::is_mounted())
    }
}
fn draw_context(){
    unsafe{
        let x=CTX_X as usize;
        let y=CTX_Y as usize;
        let w=CTX_W as usize;
        let h=(4+15*CTX_ROW+8) as usize;
        let can_sel=SEL>=0;
        let can_write=fs_writable();
        // Copy: any selected file on AetherFS / NTFS / FAT
        let can_copy=can_sel&&(VIEW==VIEW_ROOT||VIEW==VIEW_NTFS||VIEW==VIEW_FAT);
        // Cut / Rename / Delete: only writable AetherFS
        let can_modify=can_sel&&can_write;
        let can_paste=CLIP_MODE!=0&&can_write;
        // High-contrast popup so it is never missed on AH532 panel
        graphics::fill_rect(x+4,y+4,w,h,0x00000000);
        graphics::fill_rect(x,y,w,h,0x00FFF8F0);
        graphics::border_rect(x,y,w,h,0x00000000);
        graphics::border_rect(x+1,y+1,w-2,h-2,BLUE);
        graphics::fill_rect(x+2,y+2,4,h-4,BLUE);
        let item=|iy:i32,label:&str,en:bool|{
            let col=if en{TEXT}else{DIM};
            graphics::draw_str(x+16,(y as i32+4+iy*CTX_ROW+5) as usize,label,col);
        };
        item(0,"Open",can_sel);
        item(1,"Open with...",can_sel);
        graphics::fill_rect(x+10,(y as i32+4+2*CTX_ROW+10) as usize,w-20,1,BORDER);
        item(3,"Copy",can_copy);
        item(4,"Cut",can_modify);
        item(5,"Paste",can_paste);
        graphics::fill_rect(x+10,(y as i32+4+6*CTX_ROW+10) as usize,w-20,1,BORDER);
        item(7,"Rename",can_modify);
        item(8,"Delete",can_modify);
        graphics::fill_rect(x+10,(y as i32+4+9*CTX_ROW+10) as usize,w-20,1,BORDER);
        item(10,"Properties",can_sel||VIEW==VIEW_NTFS||VIEW==VIEW_ROOT);
        item(11,"Refresh",true);
        graphics::fill_rect(x+10,(y as i32+4+12*CTX_ROW+10) as usize,w-20,1,BORDER);
        item(13,"New folder",can_write);
        item(14,"New text file",can_write);
    }
}

fn draw_confirm(wx:usize,wy:usize,ww:usize,wh:usize){
    let x=wx+ww/2-120;let y=wy+wh/2-45;
    graphics::fill_rect(x,y,240,90,WHITE);
    graphics::border_rect(x,y,240,90,BORDER);
    graphics::draw_str(x+18,y+18,"Delete selected item?",TEXT);
    btn(x+24,y+52,70,"Delete",true);
    btn(x+112,y+52,70,"Cancel",false);
}

fn draw_props_dialog(wx:usize,wy:usize,ww:usize,wh:usize){
    unsafe{
        if !PROPS_OPEN{return;}
        let x=wx+ww/2-160;let y=wy+wh/2-130;
        graphics::fill_rect(x+4,y+4,320,260,0x00202020);
        graphics::fill_rect(x,y,320,260,WHITE);
        graphics::border_rect(x,y,320,260,BORDER);
        vgrad(x+1,y+1,318,30,W7_BAR_T,W7_BAR_B);
        graphics::draw_str(x+12,y+10,"Properties",TEXT);
        btn(x+272,y+4,36,"Close",false);
        if PROPS.kind==2{icon::blit(icon::IconId::Folder,x+16,y+48,false);}else{icon::blit(icon::IconId::File,x+16,y+48,false);}
        let mut k=0;while k<PROPS.name_len&&k<32{graphics::draw_char(x+54+k*8,y+55,PROPS.name[k],TEXT);k+=1;}
        graphics::fill_rect(x+14,y+82,292,1,W7_SEP);
        let mut yy=y+94;
        let mut b=Buf::new();
        b.s(if PROPS.kind==2{"Folder"}else{"File"});prow(x+14,&mut yy,"Type:",&b);
        b=Buf::new();b.size(PROPS.size);prow(x+14,&mut yy,"Size:",&b);
        b=Buf::new();b.size(PROPS.alloc);prow(x+14,&mut yy,"Size on disk:",&b);
        if PROPS.kind==2{
            b=Buf::new();b.u(PROPS.files as u64);prow(x+14,&mut yy,"Files:",&b);
            b=Buf::new();b.u(PROPS.dirs as u64);prow(x+14,&mut yy,"Folders:",&b);
        }else if PROPS.mft!=0{
            b=Buf::new();b.u(PROPS.mft as u64);prow(x+14,&mut yy,"MFT:",&b);
        }
        if PROPS.modified!=0{
            graphics::draw_str(x+14,yy,"Modified:",DIM);draw_mtime(x+124,yy,PROPS.modified);yy+=18;
        }else{
            graphics::draw_str(x+14,yy,"Dates:",DIM);graphics::draw_str(x+124,yy,"Not available yet",TEXT);yy+=18;
        }
        graphics::draw_str(x+14,yy,"Attributes:",DIM);
        graphics::draw_str(x+124,yy,if PROPS.attrs==0{"None reported"}else{"Present"},TEXT);
    }
}


fn log(s:&[u8]){serial::write_str("[EXPLORER] ");if let Ok(v)=core::str::from_utf8(s){serial::write_str(v);}serial::write_str("\n");}
pub fn open_media_folder(){
    unsafe{
        // Navigate AetherFS to /MEDIA so user sees MOZART.MP3
        CWD[0]=b'/'; CWD[1]=b'M'; CWD[2]=b'E'; CWD[3]=b'D'; CWD[4]=b'I'; CWD[5]=b'A';
        CWD_LEN=6; VIEW=VIEW_ROOT; SEL=-1;
        status(b"Open /MEDIA - double-click MOZART.MP3");
    }
}
pub fn status(s:&[u8]){log(s);unsafe{let mut i=0;while i<64{STATUS[i]=0;i+=1;}i=0;while i<s.len()&&i<63{STATUS[i]=s[i];i+=1;}STATUS_LEN=i;}}
fn put_str(b:&mut [u8;64],p:&mut usize,s:&[u8]){let mut i=0;while i<s.len()&&*p<63{b[*p]=s[i];*p+=1;i+=1;}}
fn put_num(b:&mut [u8;64],p:&mut usize,mut n:usize){let mut d=[0u8;10];let mut c=0;if n==0{d[0]=b'0';c=1;}while n>0&&c<10{d[c]=(n%10)as u8+b'0';n/=10;c+=1;}while c>0&&*p<63{c-=1;b[*p]=d[c];*p+=1;}}
fn trace_ntfs(tag:&[u8]){let n=fs_ntfs::entry_count();let mut b=[0u8;64];let mut p=0usize;put_str(&mut b,&mut p,tag);put_str(&mut b,&mut p,b" n=");put_num(&mut b,&mut p,n);let mut i=0usize;while i<3&&i<n{if let Some(e)=fs_ntfs::entry(i){put_str(&mut b,&mut p,b" ");put_num(&mut b,&mut p,i);put_str(&mut b,&mut p,b"=");let mut k=0;while k<e.name_len&&k<12&&p<62{put_str(&mut b,&mut p,&e.name[k..k+1]);k+=1;}}i+=1;}status(&b[..p]);}
fn trace_partition_click(mx:i32,my:i32,row:i32,part_no:i32){let mut b=[0u8;64];let mut p=0usize;put_str(&mut b,&mut p,b"PCLICK mx=");put_num(&mut b,&mut p,mx.max(0) as usize);put_str(&mut b,&mut p,b" my=");put_num(&mut b,&mut p,my.max(0) as usize);put_str(&mut b,&mut p,b" row=");put_num(&mut b,&mut p,row.max(0) as usize);put_str(&mut b,&mut p,b" part=");put_num(&mut b,&mut p,part_no.max(0) as usize);status(&b[..p]);}
const ROW_H:i32=32;
const ROW_TOP:i32=30;
fn hit_row(list_top:i32,my:i32)->i32{if my<list_top+ROW_TOP{-1}else{(my-list_top-ROW_TOP)/ROW_H}}
fn ntfs_visible(e:&fs_ntfs::NtfsEntry)->bool{if e.name_len==0{return false;}if e.name[0]==b'.'||e.name[0]==b'$'{return false;}true}
fn ntfs_cmp(a:usize,b:usize)->core::cmp::Ordering{
    let ea=match fs_ntfs::entry(a){Some(v)=>v,None=>return core::cmp::Ordering::Equal};
    let eb=match fs_ntfs::entry(b){Some(v)=>v,None=>return core::cmp::Ordering::Equal};
    // folders always first
    if ea.is_dir!=eb.is_dir{return if ea.is_dir{core::cmp::Ordering::Less}else{core::cmp::Ordering::Greater};}
    let mode=unsafe{SORT_MODE};
    if mode==1{ // size
        return ea.size.cmp(&eb.size);
    }
    if mode==2{ // date
        return ea.mtime.cmp(&eb.mtime);
    }
    // name
    let mut i=0usize;
    while i<ea.name_len&&i<eb.name_len{
        let mut ca=ea.name[i]; let mut cb=eb.name[i];
        if ca>=b'a'&&ca<=b'z'{ca-=32;} if cb>=b'a'&&cb<=b'z'{cb-=32;}
        if ca<cb{return core::cmp::Ordering::Less;}
        if ca>cb{return core::cmp::Ordering::Greater;}
        i+=1;
    }
    ea.name_len.cmp(&eb.name_len)
}
fn rebuild_ntfs_view(){unsafe{
    let n=fs_ntfs::entry_count();
    NTFS_VIEW_COUNT=0;
    let mut i=0usize;
    while i<n&&NTFS_VIEW_COUNT<256{
        if let Some(e)=fs_ntfs::entry(i){
            if ntfs_visible(&e){
                NTFS_VIEW_IDX[NTFS_VIEW_COUNT]=i as u8;
                NTFS_VIEW_COUNT+=1;
            }
        }
        i+=1;
    }
    // Insertion sort (folders first via ntfs_cmp)
    let mut i=1usize;
    while i<NTFS_VIEW_COUNT{
        let key=NTFS_VIEW_IDX[i];
        let mut j=i;
        while j>0{
            let prev=NTFS_VIEW_IDX[j-1] as usize;
            if ntfs_cmp(key as usize,prev)!=core::cmp::Ordering::Less{break;}
            NTFS_VIEW_IDX[j]=NTFS_VIEW_IDX[j-1];
            j-=1;
        }
        NTFS_VIEW_IDX[j]=key;
        i+=1;
    }
    // Do NOT force scroll to 0 - only clamp if past end
    if NTFS_SCROLL>=NTFS_VIEW_COUNT && NTFS_VIEW_COUNT>0{
        NTFS_SCROLL=NTFS_VIEW_COUNT-1;
    }
    if NTFS_VIEW_COUNT==0{NTFS_SCROLL=0;}
    NTFS_VIEW_DIRTY=false;
}}
fn ntfs_row_index(row:usize)->Option<usize>{unsafe{if NTFS_VIEW_DIRTY{rebuild_ntfs_view();}if row<NTFS_VIEW_COUNT{Some(NTFS_VIEW_IDX[row]as usize)}else{None}}}
fn row_h(_view:u8)->i32{32}
fn visible_rows(h:usize)->usize{h.saturating_sub(48)/row_h(VIEW_NTFS) as usize}
pub fn on_escape()->bool{unsafe{if PROPS_OPEN{PROPS_OPEN=false;return true;}if CTX{CTX=false;return true;}false}}
fn status_sel_msg(tag: &[u8]) {
    unsafe {
        let mut b = [0u8; 64];
        let mut p = 0usize;
        put_str(&mut b, &mut p, tag);
        put_str(&mut b, &mut p, b" SEL=");
        if SEL < 0 {
            put_str(&mut b, &mut p, b"-");
        } else {
            put_num(&mut b, &mut p, SEL as usize);
        }
        put_str(&mut b, &mut p, b" SCR=");
        put_num(&mut b, &mut p, NTFS_SCROLL);
        put_str(&mut b, &mut p, b" N=");
        put_num(&mut b, &mut p, NTFS_VIEW_COUNT);
        status(&b[..p]);
    }
}

pub fn on_nav_key(sc:u8)->bool{unsafe{
    if sc==0x53 { // Delete
        if SEL>=0 && fs_writable() {
            CONFIRM_DEL=true;
            status(b"Confirm delete (click Delete)");
            return true;
        }
    }
    if sc==0x3F { // F5 Refresh
        if VIEW==VIEW_FAT { let _=fs_fat::list_cwd(); }
        else if VIEW==VIEW_NTFS {
            if NTFS_CWD_REF != 0 && NTFS_CWD_REF != 0xFFFF_FFFF {
                if fs_ntfs::list_directory(NTFS_CWD_REF){NTFS_VIEW_DIRTY=true;NTFS_SCROLL=0;}
            }
        } else if VIEW==VIEW_ROOT {
            // force re-list via status only - list is on demand in draw
        }
        status(b"Refreshed");
        return true;
    }
    if sc==0x3C { // F2 Rename
        if SEL>=0 && fs_writable() { rename_selected(); return true; }
    }
    if sc==0x3D { // F3 Copy
        if SEL>=0 { clipboard_set_from_selection(false); return true; }
    }
    if sc==0x3E { // F4 Cut
        if SEL>=0 && fs_writable() { clipboard_set_from_selection(true); return true; }
    }
    if sc==0x40 { // F6 Paste
        clipboard_paste(); return true;
    }
    if sc==0x41 { // F7 New folder
        new_folder(); return true;
    }

    LAST_SC = sc;
    hint_full();
    if VIEW==VIEW_NTFS && NTFS_VIEW_DIRTY{rebuild_ntfs_view();}
    let rows=if NTFS_VISIBLE_ROWS==0 {1} else {NTFS_VISIBLE_ROWS};
    let filled = if NTFS_VIEW_COUNT<=NTFS_SCROLL {0} else {
        let left=NTFS_VIEW_COUNT-NTFS_SCROLL;
        if left<rows{left}else{rows}
    };
    // PS/2 Up=0x48; Numpad + =0x4E (AH532 arrows broken - use keypad).
    if sc==0x48 || sc==0x49 || sc==0x47 || sc==0x4E {
        LAST_NAV_TAG = 1;
        if VIEW==VIEW_NTFS {
            if NTFS_VIEW_COUNT==0 { fill_status_bar(); return true; }
            if SEL < 0 {
                SEL = 0;
            } else if SEL > 0 {
                SEL -= 1;
            } else if NTFS_SCROLL > 0 {
                NTFS_SCROLL -= 1;
                SEL = 0;
            } else {
                SEL = 0;
            }
            fill_status_bar();
            return true;
        }
        if SEL > 0 { SEL -= 1; } else { SEL = 0; }
        if VIEW==VIEW_COMPUTER {
            if COMPUTER_SEL > 0 { COMPUTER_SEL -= 1; } else { COMPUTER_SEL = 0; }
            SEL = COMPUTER_SEL;
        }
        fill_status_bar();
        return true;
    }
    if sc==0x50 || sc==0x51 || sc==0x4F || sc==0x4A {
        LAST_NAV_TAG = 2;
        if VIEW==VIEW_NTFS {
            if NTFS_VIEW_COUNT==0 { fill_status_bar(); return true; }
            if SEL < 0 {
                SEL = 0;
                fill_status_bar();
                return true;
            }
            let abs = NTFS_SCROLL + (SEL as usize);
            if abs + 1 >= NTFS_VIEW_COUNT {
                if filled > 0 { SEL = (filled - 1) as i32; }
                fill_status_bar();
                return true;
            }
            if (SEL as usize) + 1 < filled {
                SEL += 1;
            } else if NTFS_SCROLL + filled < NTFS_VIEW_COUNT {
                NTFS_SCROLL += 1;
                SEL = (filled.saturating_sub(1)) as i32;
            }
            fill_status_bar();
            return true;
        }
        SEL += 1;
        if VIEW==VIEW_COMPUTER {
            COMPUTER_SEL += 1;
            if COMPUTER_SEL > 16 { COMPUTER_SEL = 16; }
            SEL = COMPUTER_SEL;
        }
        fill_status_bar();
        return true;
    }
    if sc==0x4B{ // Left = Up (all folder views)
        if VIEW==VIEW_NTFS || VIEW==VIEW_FAT || VIEW==VIEW_ROOT || VIEW==VIEW_DISK || VIEW==VIEW_TEXT {
            go_up();
            return true;
        }
        return false;
    }
    if sc==0x4D{ // Right - open selected folder on Enter-like
        if VIEW==VIEW_NTFS && SEL>=0{
            // fall through to Enter handling below via fake
            let idx=match ntfs_row_index(NTFS_SCROLL+(SEL as usize)){Some(v)=>v,None=>return true};
            if let Some(e)=fs_ntfs::entry(idx){
                if e.is_dir{
                    if fs_ntfs::list_directory(e.mft_ref){
                        NTFS_VIEW_DIRTY=true;NTFS_SCROLL=0;
                        if NTFS_DEPTH<8{
                            NTFS_PARENT[NTFS_DEPTH]=NTFS_CWD_REF;
                            let mut k=0usize; while k<28{NTFS_SEG[NTFS_DEPTH][k]=0;k+=1;}
                            k=0; while k<e.name_len&&k<27{NTFS_SEG[NTFS_DEPTH][k]=e.name[k];k+=1;}
                            NTFS_SEG_LEN[NTFS_DEPTH]=k; NTFS_DEPTH+=1;
                        }
                        NTFS_CWD_REF=e.mft_ref; SEL=-1; status(b"Opened (NTFS, max 256 listed)");
                    }
                }
            }
            return true;
        }
        return false;
    }
    if sc==0x49{ // PgUp
        if NTFS_SCROLL>=rows{NTFS_SCROLL-=rows;}else{NTFS_SCROLL=0;}
        return true;
    }
    if sc==0x51{ // PgDn
        let max_s=NTFS_VIEW_COUNT.saturating_sub(rows);
        NTFS_SCROLL=if NTFS_SCROLL+rows>max_s{max_s}else{NTFS_SCROLL+rows};
        return true;
    }
    if sc==0x1C || sc==0x0E{ // Enter / Backspace as Up
        if sc==0x0E{ go_up(); return true; }
        if SEL>=0{
            let idx=match ntfs_row_index(NTFS_SCROLL+(SEL as usize)){Some(v)=>v,None=>return true};
            if let Some(e)=fs_ntfs::entry(idx){
                if e.is_dir{
                    if fs_ntfs::list_directory(e.mft_ref){
                        NTFS_VIEW_DIRTY=true;NTFS_SCROLL=0;
                        if NTFS_DEPTH<8{
                            NTFS_PARENT[NTFS_DEPTH]=NTFS_CWD_REF;
                            let mut k=0usize; while k<28{NTFS_SEG[NTFS_DEPTH][k]=0;k+=1;}
                            k=0; while k<e.name_len&&k<27{NTFS_SEG[NTFS_DEPTH][k]=e.name[k];k+=1;}
                            NTFS_SEG_LEN[NTFS_DEPTH]=k;
                            NTFS_DEPTH+=1;
                        }
                        NTFS_CWD_REF=e.mft_ref; SEL=-1; status(b"Opened");
                    }
                }else{
                    status(b"File selected (Enter)");
                }
            }
        }
        return true;
    }
    false
}}
fn set_root(){unsafe{CWD[0]=b'/';CWD_LEN=1;VIEW=VIEW_ROOT;SEL=-1;}}
fn push_hist(){unsafe{if HIST_N<8{let mut i=0;while i<32{HIST[HIST_N][i]=CWD[i];i+=1;}HIST_LEN[HIST_N]=CWD_LEN;HIST_N+=1;HIST_I=HIST_N;}}}

fn file_type_label(name:&[u8], nlen:usize, is_dir:bool)->&'static str{
    if is_dir{return "File folder";}
    let mut dot=None;
    let mut i=0usize;
    while i<nlen{
        if name[i]==b'.'{dot=Some(i);}
        i+=1;
    }
    if let Some(d)=dot{
        let mut ext=[0u8;8]; let mut el=0usize; let mut j=d+1;
        while j<nlen && el<8{
            let c=name[j];
            ext[el]=if c>=b'a'&&c<=b'z'{c-32}else{c};
            el+=1; j+=1;
        }
        match &ext[..el]{
            b"TXT"|b"LOG"|b"INI"|b"CFG"|b"MD" => "Text Document",
            b"EXE"|b"COM" => "Application",
            b"DLL"|b"SYS" => "System file",
            b"JPG"|b"JPEG"|b"PNG"|b"BMP"|b"GIF" => "Image",
            b"MP3"|b"WAV"|b"WMA"|b"FLAC" => "Audio",
            b"MP4"|b"M4V"|b"MOV"|b"AVI"|b"MJPG"|b"MJPEG" => "Video",
            b"MP4"|b"AVI"|b"MKV"|b"WMV" => "Video",
            b"PDF" => "PDF Document",
            b"DOC"|b"DOCX"|b"RTF" => "Document",
            b"ZIP"|b"RAR"|b"7Z"|b"CAB" => "Compressed",
            b"ISO"|b"IMG" => "Disc Image File",
            b"HTM"|b"HTML" => "HTML Document",
            b"BAT"|b"CMD" => "Windows Batch",
            _ => "File",
        }
    }else{"File"}
}

fn draw_num(x:usize,y:usize,mut n:u32){if n==0{graphics::draw_char(x,y,b'0',TEXT);return;}let mut d=[0u8;10];let mut c=0;while n>0{d[c]=(n%10)as u8+b'0';n/=10;c+=1;}let start=c;while c>0{c-=1;graphics::draw_char(x+(start-1-c)*8,y,d[c],TEXT);}}
/// Win7-style size: auto unit B / KB / MB / GB from real byte count.
fn draw_size_kb(x:usize,y:usize,bytes:u64){
    if bytes==0{
        graphics::draw_str(x,y,"0 KB",DIM);
        return;
    }
    // Choose unit so the integer part stays readable
    let (val, unit): (u64, &str) = if bytes < 1024 {
        (bytes, "B")
    } else if bytes < 1024 * 1024 {
        ((bytes + 512) / 1024, "KB")
    } else if bytes < 1024 * 1024 * 1024 {
        ((bytes + 512 * 1024) / (1024 * 1024), "MB")
    } else {
        ((bytes + 512 * 1024 * 1024) / (1024 * 1024 * 1024), "GB")
    };
    // Format integer with thin spaces every 3 digits (from the right)
    let mut d=[0u8;20];
    let mut c=0usize;
    let mut n=val;
    if n==0{d[0]=b'0';c=1;}
    while n>0 && c<20{
        d[c]=(n%10) as u8 + b'0';
        n/=10;
        c+=1;
    }
    let mut buf=[0u8;24];
    let mut p=0usize;
    let mut i=0usize;
    while i<c{
        if i>0 && i%3==0 && p<23{
            buf[p]=b' ';
            p+=1;
        }
        buf[p]=d[c-1-i];
        p+=1;
        i+=1;
    }
    let mut j=0usize;
    while j<p{
        graphics::draw_char(x+j*8,y,buf[j],TEXT);
        j+=1;
    }
    graphics::draw_str(x+p*8+8,y,unit,DIM);
}
fn ft_ymdhm(ft:u64)->(u32,u32,u32,u32,u32){
    // NTFS FILETIME: 100ns since 1601-01-01. Guard zero/garbage.
    if ft<10_000_000{return (0,0,0,0,0);}
    let secs=(ft/10_000_000).saturating_sub(11_644_473_600);
    let days=(secs/86400) as i64; let rem=(secs%86400) as u32;
    let z=days+719468; let era=z.div_euclid(146097); let doe=z-era*146097;
    let yoe=(doe-doe/1460+doe/36524-doe/146096)/365;
    let doy=doe-(365*yoe+yoe/4-yoe/100); let mp=(5*doy+2)/153;
    let d=(doy-(153*mp+2)/5+1) as u32; let m=(if mp<10{mp+3}else{mp-9}) as u32;
    let y=(yoe+era*400+if m<=2{1}else{0}) as u32;
    if y<1980||y>2100{return (0,0,0,0,0);}
    (y,m,d,rem/3600,(rem%3600)/60)
}
fn draw_2(x:usize,y:usize,v:u32){
    graphics::draw_char(x,y,b'0'+((v/10)%10) as u8,TEXT);
    graphics::draw_char(x+8,y,b'0'+(v%10) as u8,TEXT);
}
/// Win7-style: DD.MM.YYYY HH:MM
fn draw_mtime(x:usize,y:usize,ft:u64){
    let (yr,mo,d,hh,mi)=ft_ymdhm(ft);
    if yr==0{graphics::draw_str(x,y,"-",DIM);return;}
    draw_2(x,y,d);
    graphics::draw_char(x+16,y,b'.',TEXT);
    draw_2(x+24,y,mo);
    graphics::draw_char(x+40,y,b'.',TEXT);
    let mut q=yr; let mut digs=[0u8;4]; let mut i=4usize;
    while i>0{i-=1;digs[i]=b'0'+(q%10) as u8;q/=10;}
    graphics::draw_char(x+48,y,digs[0],TEXT);
    graphics::draw_char(x+56,y,digs[1],TEXT);
    graphics::draw_char(x+64,y,digs[2],TEXT);
    graphics::draw_char(x+72,y,digs[3],TEXT);
    graphics::draw_char(x+88,y,b' ',TEXT);
    draw_2(x+96,y,hh);
    graphics::draw_char(x+112,y,b':',TEXT);
    draw_2(x+120,y,mi);
}
fn btn(x:usize,y:usize,w:usize,label:&str,hot:bool){
    if hot{vgrad(x,y,w,24,0x00EAF6FD,0x00A7D9F5);}else{vgrad(x,y,w,24,0x00F6F7F9,0x00E3E8EF);}
    graphics::border_rect(x,y,w,24,if hot{0x003C7FB1}else{0x00A0A8B4});
    graphics::draw_str(x+8,y+8,label,TEXT);
}

pub fn reset(){unsafe{VIEW=VIEW_COMPUTER;SEL=-1;HOVER=-1;COMPUTER_SEL=-1;FOCUS_ADDR=false;CTX=false;CONFIRM_DEL=false;CLIP_MODE=0;CLIP_NAME_LEN=0;CLIP_PATH_LEN=0;CLIP_DATA_LEN=0;CLIP_IS_DIR=false;HIST_N=0;HIST_I=0;CWD[0]=b'/';CWD_LEN=1;NTFS_CWD_REF=5;NTFS_DEPTH=0;NTFS_VIEW_COUNT=0;NTFS_SCROLL=0;NTFS_VISIBLE_ROWS=1;NTFS_VIEW_DIRTY=true;PREVIEW_LEN=0;PREVIEW_FROM=VIEW_COMPUTER;PROPS_OPEN=false;}status(b"Ready");}

fn draw_header(wx:usize,wy:usize,ww:usize,body_y:usize){
    // Win7: glass toolbar + address bar + search + command strip
    vgrad(wx+3,body_y,ww.saturating_sub(6),56,0x00FBFCFE,0x00E8EEF5);
    graphics::fill_rect(wx+3,body_y+55,ww.saturating_sub(6),1,0x00B0B8C4);

    // Round-ish Back / Forward (filled circles approx)
    graphics::fill_rect(wx+10,body_y+5,28,28,0x00DCE8F4);
    graphics::border_rect(wx+10,body_y+5,28,28,0x006A90B8);
    graphics::fill_rect(wx+11,body_y+6,26,1,WHITE);
    graphics::draw_str(wx+19,body_y+14,"<",W7_NAV);
    graphics::fill_rect(wx+42,body_y+5,28,28,0x00DCE8F4);
    graphics::border_rect(wx+42,body_y+5,28,28,0x006A90B8);
    graphics::fill_rect(wx+43,body_y+6,26,1,WHITE);
    graphics::draw_str(wx+51,body_y+14,">",W7_NAV);
    btn(wx+74,body_y+7,30,"Up",false);

    let search_w=120usize.min(ww.saturating_sub(240));
    let ax=wx+112;
    let aw=ww.saturating_sub(124+search_w);
    // Address bar (white, inset)
    graphics::fill_rect(ax,body_y+5,aw,28,WHITE);
    graphics::border_rect(ax,body_y+5,aw,28,0x007A9BC0);
    graphics::fill_rect(ax+1,body_y+6,aw.saturating_sub(2),1,0x00E8E8E8);
    unsafe{
        let mut cx = ax+8;
        let max_x = ax + aw.saturating_sub(8);
        graphics::draw_str(cx, body_y+14, "Computer", W7_NAV);
        cx += 8*8;
        if VIEW==VIEW_DISK || VIEW==VIEW_NTFS || VIEW==VIEW_FAT{
            if cx+24 < max_x { graphics::draw_str(cx, body_y+14, " > ", DIM); cx += 24; }
            if cx+12*8 < max_x { graphics::draw_str(cx, body_y+14, "Local Disk (C:)", TEXT); cx += 15*8; }
        }
        if VIEW==VIEW_NTFS{
            let mut d=0usize;
            while d<NTFS_DEPTH && cx+16 < max_x {
                graphics::draw_str(cx, body_y+14, " > ", DIM); cx += 24;
                let nlen=NTFS_SEG_LEN[d];
                if nlen>0 && cx+8 < max_x {
                    draw_name_soft(cx, body_y+14, &NTFS_SEG[d], nlen, ((max_x-cx)/8).min(24), TEXT);
                    cx += nlen.min(24)*8;
                }
                d += 1;
            }
        }else if VIEW==VIEW_ROOT{
            if cx+24 < max_x { graphics::draw_str(cx, body_y+14, " > ", DIM); cx += 24; }
            graphics::draw_str(cx, body_y+14, "AetherFS", TEXT);
            if CWD_LEN>1 && cx+16 < max_x {
                graphics::draw_str(cx, body_y+14, " > ", DIM); cx += 24;
                draw_name_soft(cx, body_y+14, &CWD[1..], CWD_LEN.saturating_sub(1), ((max_x-cx)/8).min(20), TEXT);
            }
        }else if VIEW==VIEW_FAT{
            if cx+24 < max_x { graphics::draw_str(cx, body_y+14, " > ", DIM); cx += 24; }
            if fs_fat::at_root() {
                graphics::draw_str(cx, body_y+14, "FAT (root)", TEXT);
            } else {
                graphics::draw_str(cx, body_y+14, "FAT (folder)", TEXT);
            }
        }
    }

    let sx=wx+ww.saturating_sub(search_w+10);
    graphics::fill_rect(sx,body_y+5,search_w,28,WHITE);
    graphics::border_rect(sx,body_y+5,search_w,28,0x007A9BC0);
    graphics::draw_str(sx+8,body_y+14,"Search Computer",DIM);

    // Command bar - real actions (click handled in on_click)
    vgrad(wx+3,body_y+34,ww.saturating_sub(6),20,0x00F5F8FC,0x00E6ECF4);
    graphics::draw_str(wx+12,body_y+40,"Organize",W7_NAV);
    graphics::draw_str(wx+88,body_y+40,"New folder",W7_NAV);
    graphics::draw_str(wx+184,body_y+40,"New file",W7_NAV);
    graphics::draw_str(wx+268,body_y+40,"Paste",W7_NAV);
    graphics::draw_str(wx+320,body_y+40,"Refresh",W7_NAV);
    let _=wy;
}
fn draw_sidebar(wx:usize,y:usize,w:usize,h:usize){
    // Win7 navigation pane
    vgrad(wx+3,y,w,h,0x00F4F8FC,0x00DDE6F0);
    graphics::border_rect(wx+3,y,w,h,0x00A8B4C4);
    graphics::fill_rect(wx+w+2,y,1,h,0x00C8D0D8);

    let mut yy=y+8;
    graphics::draw_str(wx+14,yy,"Favorites",W7_NAV); yy+=18;
    icon::blit(icon::IconId::Folder,wx+14,yy,false);
    graphics::draw_str(wx+46,yy+8,"Desktop",TEXT); yy+=26;
    icon::blit(icon::IconId::Folder,wx+14,yy,false);
    graphics::draw_str(wx+46,yy+8,"Downloads",TEXT); yy+=26;
    icon::blit(icon::IconId::Folder,wx+14,yy,false);
    graphics::draw_str(wx+46,yy+8,"Recent places",TEXT); yy+=30;

    graphics::draw_str(wx+14,yy,"Libraries",W7_NAV); yy+=18;
    icon::blit(icon::IconId::Folder,wx+14,yy,false);
    graphics::draw_str(wx+46,yy+8,"Documents",TEXT); yy+=26;
    icon::blit(icon::IconId::Folder,wx+14,yy,false);
    graphics::draw_str(wx+46,yy+8,"Music",TEXT); yy+=26;
    icon::blit(icon::IconId::Folder,wx+14,yy,false);
    graphics::draw_str(wx+46,yy+8,"Pictures",TEXT); yy+=30;

    graphics::draw_str(wx+14,yy,"Computer",W7_NAV); yy+=18;
    icon::blit(icon::IconId::MyComputer,wx+14,yy,false);
    graphics::draw_str(wx+46,yy+8,"Local Disk (C:)",TEXT); yy+=26;
    icon::blit(icon::IconId::MyComputer,wx+14,yy,false);
    graphics::draw_str(wx+46,yy+8,"AetherFS (A:)",TEXT); yy+=30;

    graphics::draw_str(wx+14,yy,"Network",W7_NAV); yy+=18;
    icon::blit(icon::IconId::Network,wx+14,yy,false);
    graphics::draw_str(wx+46,yy+8,"Network",TEXT);
    let _=h;
}
fn draw_drive(wx:usize,y:usize,name:&str,sub:&str,sel:bool){
    if sel{
        graphics::fill_rect(wx,y,280,76,SELECT);
        graphics::border_rect(wx,y,280,76,0x0026A0DA);
    }else{
        graphics::fill_rect(wx,y,280,76,WHITE);
        graphics::border_rect(wx,y,280,76,0x00D8D8D8);
    }
    icon::blit(icon::IconId::MyComputer,wx+14,y+12,false);
    graphics::draw_str(wx+56,y+14,name,TEXT);
    graphics::draw_str(wx+56,y+32,sub,DIM);
    // capacity bar (Win7 style)
    graphics::fill_rect(wx+56,y+52,200,10,0x00E8E8E8);
    graphics::fill_rect(wx+56,y+52,110,10,0x0060A0E0);
    graphics::border_rect(wx+56,y+52,200,10,0x00A0A0A0);
}
fn draw_computer(wx:usize,y:usize,ww:usize,h:usize){
    graphics::draw_str(wx+4,y+8,"Computer",W7_NAV);
    graphics::draw_str(wx+4,y+28,"Hard Disk Drives",DIM);

    // Render every physical disk discovered by the block layer. Do not collapse
    // multiple disks into a single "C:" card.
    let mut disks=[false;8];
    let mut dn=0usize;
    let pn=part::count();
    let mut i=0usize;
    while i<pn{
        if let Some(p)=part::get(i){
            let d=p.disk as usize;
            if d<8&&!disks[d]{disks[d]=true;dn+=1;}
        }
        i+=1;
    }

    let mut d=0usize;
    let mut row=0usize;
    while d<8{
        if disks[d]{
            let cy=y+45+row*82;
            let selected=unsafe{COMPUTER_SEL==d as i32};
            if selected{graphics::fill_rect(wx,cy,ww,74,SELECT);}else{graphics::fill_rect(wx,cy,ww,74,WHITE);}
            graphics::border_rect(wx,cy,ww,74,if selected{0x007DA2CE}else{BORDER});
            icon::blit(icon::IconId::MyComputer,wx+10,cy+11,false);

            if d==0{
                graphics::draw_str(wx+52,cy+12,"Disk 0",TEXT);
            }else if d==1{
                graphics::draw_str(wx+52,cy+12,"Disk 1",TEXT);
            }else{
                graphics::draw_str(wx+52,cy+12,"Disk",TEXT);
            }

            let mut parts_on_disk=0usize;
            let mut total_mb=0u32;
            let mut j=0usize;
            while j<pn{
                if let Some(p)=part::get(j){
                    if p.disk as usize==d{
                        parts_on_disk+=1;
                        total_mb=total_mb.saturating_add(p.sectors/2048);
                    }
                }
                j+=1;
            }
            graphics::draw_str(wx+52,cy+30,"Partitions:",DIM);
            draw_num(wx+132,cy+30,parts_on_disk as u32);
            graphics::draw_str(wx+145,cy+30,"-",DIM);
            draw_num(wx+166,cy+30,total_mb);
            graphics::draw_str(wx+214,cy+30,"MB",DIM);
            graphics::fill_rect(wx+52,cy+50,ww.saturating_sub(68),8,TOOL2);
            graphics::border_rect(wx+52,cy+50,ww.saturating_sub(68),8,BORDER);
            row+=1;
        }
        d+=1;
    }

    let ay=y+45+row*82;
    let selected_a=unsafe{COMPUTER_SEL==8};
    if selected_a{graphics::fill_rect(wx,ay,ww,74,SELECT);}else{graphics::fill_rect(wx,ay,ww,74,WHITE);}
    graphics::border_rect(wx,ay,ww,74,if selected_a{0x007DA2CE}else{BORDER});
    icon::blit(icon::IconId::MyComputer,wx+10,ay+11,false);
    graphics::draw_str(wx+52,ay+12,"AetherFS (A:)",TEXT);
    graphics::draw_str(wx+52,ay+30,"RAM filesystem • Read/Write",DIM);
    graphics::fill_rect(wx+52,ay+50,ww.saturating_sub(68),8,TOOL2);

    // USB Phone (Mass Storage)
    let py=ay+82;
    let phone_on=crate::drivers::usb_phone::is_connected();
    let phone_mnt=crate::drivers::usb_phone::is_mounted();
    let selected_p=unsafe{COMPUTER_SEL==9};
    if selected_p{graphics::fill_rect(wx,py,ww,74,SELECT);}else{graphics::fill_rect(wx,py,ww,74,WHITE);}
    graphics::border_rect(wx,py,ww,74,if selected_p{0x007DA2CE}else{BORDER});
    icon::blit(icon::IconId::Network,wx+10,py+11,false);
    graphics::draw_str(wx+52,py+12,"USB Phone (P:)",TEXT);
    if phone_mnt {
        let info=crate::drivers::usb_phone::info();
        graphics::draw_str(wx+52,py+30,"Mounted • Mass Storage",0x00008000);
        // size approx MB
        let mb=info.block_count.saturating_mul(info.block_size.saturating_div(1024)).saturating_div(1024);
        graphics::draw_str(wx+52,py+48,"Capacity MB:",DIM);
        draw_num(wx+140,py+48,mb);
    } else if phone_on {
        graphics::draw_str(wx+52,py+30,"Detected • not mounted",0x00800000);
        graphics::draw_str(wx+52,py+48,"Unlock phone + File transfer, then PH",DIM);
    } else {
        graphics::draw_str(wx+52,py+30,"No phone connected",DIM);
        graphics::draw_str(wx+52,py+48,"Plug USB and run PH in Terminal",DIM);
    }

    graphics::draw_str(wx,y+h.saturating_sub(18),"Double-click disk / AetherFS / USB Phone.",DIM);
}
fn draw_disk(wx:usize,y:usize,ww:usize,h:usize){
    graphics::fill_rect(wx,y,ww,h,WHITE);
    graphics::draw_str(wx+8,y+8,"Physical disk partitions",TEXT);
    let n=part::count();
    if n==0{
        graphics::draw_str(wx+10,y+34,"No partitions detected.",RED);
        return;
    }
    vgrad(wx,y,ww,28,W7_HDR_T,W7_HDR_B);
    graphics::fill_rect(wx,y+27,ww,1,W7_SEP);
    graphics::fill_rect(wx+244,y+3,1,22,W7_SEP);
    graphics::fill_rect(wx+344,y+3,1,22,W7_SEP);
    graphics::draw_str(wx+8,y+9,"Disk",TEXT);
    graphics::draw_str(wx+76,y+9,"Partition",TEXT);
    graphics::draw_str(wx+140,y+9,"Filesystem",TEXT);
    graphics::draw_str(wx+280,y+9,"Size MB",TEXT);
    unsafe{
        let mut row=0usize;
        let mut i=0usize;
        while i<n&&row<18{
            if let Some(p)=part::get(i){
                let ry=y+52+row*32;
                unsafe{
                    if SEL==row as i32{sel_row(wx+2,ry,ww-4,ROW_H as usize,false);}
                    else if HOVER==row as i32{sel_row(wx+2,ry,ww-4,ROW_H as usize,true);}
                }
                icon::blit(icon::IconId::MyComputer,wx+4,ry,false);
                draw_num(wx+44,ry+7,p.disk as u32);
                draw_num(wx+108,ry+7,p.index as u32);
                graphics::draw_str(wx+140,ry+7,part::type_name(p.ptype),DIM);
                draw_num(wx+280,ry+7,p.sectors/2048);
                row+=1;
            }
            i+=1;
        }
        graphics::draw_str(wx+8,y+h.saturating_sub(20),"Double-click a partition to open its read-only filesystem.",DIM);
    }
}
fn draw_fat(wx:usize,y:usize,ww:usize,h:usize){
    graphics::fill_rect(wx,y,ww,h,WHITE);
    vgrad(wx,y,ww,28,W7_HDR_T,W7_HDR_B);
    graphics::fill_rect(wx,y+27,ww,1,W7_SEP);
    graphics::fill_rect(wx+244,y+3,1,22,W7_SEP);
    graphics::fill_rect(wx+344,y+3,1,22,W7_SEP);
    graphics::draw_str(wx+10,y+9,"Name",TEXT);
    graphics::draw_str(wx+250,y+9,"Type",TEXT);
    graphics::draw_str(wx+350,y+9,"Size",TEXT);
    let n=fs_fat::entry_count();
    if n==0{
        graphics::draw_str(wx+16,y+52,"Folder empty or not listed yet.",DIM);
        graphics::draw_str(wx+16,y+70,"Press F5 or click Refresh to load.",W7_NAV);
        return;
    }
    let mut i=0usize;
    while i<n&&i<32&&y+ROW_TOP as usize+i*ROW_H as usize+24<=y+h{
        if let Some(e)=fs_fat::entry(i){
            let ry=y+ROW_TOP as usize+i*ROW_H as usize;
            unsafe{
                if SEL==i as i32{sel_row(wx+2,ry,ww-4,ROW_H as usize,false);}
                else if HOVER==i as i32{sel_row(wx+2,ry,ww-4,ROW_H as usize,true);}
            }
            if e.is_dir{icon::blit(icon::IconId::Folder,wx+7,ry,false);}else{icon::blit(icon::IconId::File,wx+7,ry,false);}
            let col=TEXT;
            let mut k=0;while k<e.name_len&&k<27{graphics::draw_char(wx+42+k*8,ry+10,e.name[k],col);k+=1;}
            graphics::draw_str(wx+250,ry+8,file_type_label(&e.name,e.name_len,e.is_dir),DIM);
            if !e.is_dir{draw_num(wx+350,ry+8,e.size);}
        }
        i+=1;
    }
}
fn draw_ntfs(wx:usize,y:usize,ww:usize,h:usize){
    if unsafe{NTFS_VIEW_DIRTY}{rebuild_ntfs_view();}
    graphics::fill_rect(wx,y,ww,h,WHITE);
    vgrad(wx,y,ww,28,W7_HDR_T,W7_HDR_B);
    graphics::fill_rect(wx,y+27,ww,1,W7_SEP);
    graphics::fill_rect(wx+244,y+3,1,22,W7_SEP);
    graphics::fill_rect(wx+384,y+3,1,22,W7_SEP);
    graphics::fill_rect(wx+454,y+3,1,22,W7_SEP);
    graphics::draw_str(wx+10,y+9,"Name",W7_NAV);
    graphics::draw_str(wx+250,y+9,"Date modified",W7_NAV);
    graphics::draw_str(wx+400,y+9,"Type",W7_NAV);
    graphics::draw_str(wx+520,y+9,"Size",W7_NAV);
    graphics::fill_rect(wx+244,y+3,1,22,W7_SEP);
    graphics::fill_rect(wx+394,y+3,1,22,W7_SEP);
    graphics::fill_rect(wx+514,y+3,1,22,W7_SEP);
    if unsafe{NTFS_VIEW_COUNT==0}{graphics::draw_str(wx+16,y+52,"This folder is empty or unavailable.",DIM);graphics::draw_str(wx+16,y+70,"NTFS is currently mounted read-only.",DIM);return;}
    let rows=visible_rows(h);unsafe{NTFS_VISIBLE_ROWS=if rows==0{1}else{rows};if NTFS_SCROLL+NTFS_VISIBLE_ROWS>NTFS_VIEW_COUNT{NTFS_SCROLL=NTFS_VIEW_COUNT.saturating_sub(NTFS_VISIBLE_ROWS);}}
    let mut r=0usize;
    while r<rows{
        let vi=unsafe{NTFS_SCROLL+r};
        let idx=match ntfs_row_index(vi){Some(v)=>v,None=>break};
        if let Some(e)=fs_ntfs::entry(idx){
            let ry=y+ROW_TOP as usize+r*32;
            unsafe{
                if SEL==r as i32{sel_row(wx+2,ry,ww-4,ROW_H as usize,false);}
                else if HOVER==r as i32{sel_row(wx+2,ry,ww-4,ROW_H as usize,true);}
            }
            if e.is_dir{icon::blit(icon::IconId::Folder,wx+7,ry,false);}else{icon::blit(icon::IconId::File,wx+7,ry,false);}
            let max_chars=((ww.saturating_sub(50))/8).min(36).max(12);
            draw_name_soft(wx+42,ry+10,&e.name,e.name_len,max_chars,TEXT);
            if ww>400{draw_mtime(wx+250,ry+10,e.mtime);}
            graphics::draw_str(wx+400,ry+10,file_type_label(&e.name,e.name_len,e.is_dir),DIM);
            if !e.is_dir{
                // Prefer authoritative $DATA size over stale $FILE_NAME size
                let sz = match fs_ntfs::file_data_size(e.mft_ref) {
                    Some(n) if n > 0 => n as u64,
                    _ => e.size,
                };
                draw_size_kb(wx+520,ry+10,sz);
            }
        }
        r+=1;
    }
    // Scrollbar (right edge) - coordinates must match on_click bar_x
    unsafe{
        let vis=if NTFS_VISIBLE_ROWS==0 {1} else {NTFS_VISIBLE_ROWS};
        if NTFS_VIEW_COUNT > vis {
            let bar_x=wx+ww.saturating_sub(14);
            let bar_top=y+ROW_TOP as usize;
            let bar_h=vis.saturating_mul(32);
            if bar_h>0 {
                graphics::fill_rect(bar_x, bar_top, 12, bar_h, 0x00E8E8E8);
                graphics::border_rect(bar_x, bar_top, 12, bar_h, BORDER);
                let max_scroll=NTFS_VIEW_COUNT.saturating_sub(vis);
                let thumb_h=if max_scroll==0||NTFS_VIEW_COUNT==0 {
                    bar_h
                } else {
                    let t=(bar_h.saturating_mul(vis))/NTFS_VIEW_COUNT;
                    if t<12{12}else if t>bar_h{bar_h}else{t}
                };
                let thumb_y=bar_top + if max_scroll==0 {
                    0
                } else {
                    (bar_h.saturating_sub(thumb_h)).saturating_mul(NTFS_SCROLL)/max_scroll
                };
                graphics::fill_rect(bar_x+2, thumb_y, 8, thumb_h, 0x006B9BD1);
            }
        }
    }
}
fn draw_list(wx:usize,y:usize,ww:usize,h:usize){
    if !fs::is_mounted(){graphics::draw_str(wx+10,y+10,"AetherFS is not mounted.",RED);return;}
    let mut a=[fs::ListItem{name:[0;24],name_len:0,size:0,is_dir:false};16];
    let n=unsafe{fs::list_ex_path(core::str::from_utf8_unchecked(&CWD[..CWD_LEN]),&mut a)};
    graphics::fill_rect(wx,y,ww,h,WHITE);
    vgrad(wx,y,ww,28,W7_HDR_T,W7_HDR_B);
    graphics::fill_rect(wx,y+27,ww,1,W7_SEP);
    graphics::fill_rect(wx+214,y+3,1,22,W7_SEP);
    graphics::fill_rect(wx+304,y+3,1,22,W7_SEP);
    graphics::draw_str(wx+10,y+9,"Name",TEXT);
    graphics::draw_str(wx+220,y+9,"Type",TEXT);
    graphics::draw_str(wx+310,y+9,"Size",TEXT);
    if n==0{
        graphics::draw_str(wx+16,y+52,"This folder is empty.",DIM);
        graphics::draw_str(wx+16,y+70,"Create a folder or file from the context menu.",DIM);
        return;
    }
    unsafe{
        let mut i=0;
        while i<n{
            let ry=y+ROW_TOP as usize+i*ROW_H as usize;
            if SEL==i as i32{sel_row(wx+2,ry,ww-4,ROW_H as usize,false);}
            else if HOVER==i as i32{sel_row(wx+2,ry,ww-4,ROW_H as usize,true);}
            if a[i].is_dir{icon::blit(icon::IconId::Folder,wx+7,ry,false);}else{icon::blit(icon::IconId::File,wx+7,ry,false);}
            draw_name_soft(wx+42,ry+8,&a[i].name,a[i].name_len,22,TEXT);
            graphics::draw_str(wx+220,ry+8,if a[i].is_dir{"File folder"}else{"File"},DIM);
            if !a[i].is_dir{draw_num(wx+310,ry+8,a[i].size);}
            i+=1;
        }
    }
}
fn draw_text(wx:usize,y:usize,ww:usize,h:usize){
    graphics::fill_rect(wx,y,ww,h,WHITE);
    vgrad(wx,y,ww,28,W7_HDR_T,W7_HDR_B);
    graphics::fill_rect(wx,y+27,ww,1,W7_SEP);
    graphics::draw_str(wx+10,y+9,"File Preview",TEXT);
    unsafe{
        let mut info=[0u8;64]; let mut p=0usize;
        put_str(&mut info,&mut p,b"Bytes: ");
        put_num(&mut info,&mut p,PREVIEW_LEN);
        put_str(&mut info,&mut p,b"  (Back = return)");
        let mut i=0; while i<p{ graphics::draw_char(wx+160+i*8,y+9,info[i],DIM); i+=1; }
        let mut i=0; let mut col=0; let mut row=0;
        while i<PREVIEW_LEN{
            let c=PREVIEW[i];
            if c==b'\n' || col> (ww.saturating_sub(24)/8).max(40) {
                row+=1; col=0;
                if c==b'\n'{ i+=1; continue; }
            }
            // printable
            let ch = if c>=32 && c<127 { c } else if c==b'\t' { b' ' } else { b'.' };
            if y+36+row*12 < y+h.saturating_sub(8) {
                graphics::draw_char(wx+10+col*8, y+36+row*12, ch, TEXT);
            }
            col+=1; i+=1;
        }
        if PREVIEW_LEN==0{
            graphics::draw_str(wx+16,y+52,"No preview data.",DIM);
        }
    }
}



pub fn draw(wx:usize,wy:usize,ww:usize,wh:usize,title_h:usize){
    let by=wy+title_h;
    graphics::fill_rect(wx+3,by,ww.saturating_sub(6),wh.saturating_sub(title_h+3),WHITE);
    draw_header(wx,wy,ww,by);
    // Header 56px; details strip 28px at bottom
    let details_h=28usize;
    let sy=by+56;
    let sh=wh.saturating_sub(title_h+56+details_h+4);
    let side=168usize.min(ww/3);
    draw_sidebar(wx,sy,side,sh);
    let cx=wx+side+8;let cw=ww.saturating_sub(side+16);
    match unsafe{VIEW}{
        VIEW_COMPUTER=>draw_computer(cx,sy,cw,sh),
        VIEW_ROOT=>draw_list(cx,sy,cw,sh),
        VIEW_DISK=>draw_disk(cx,sy,cw,sh),
        VIEW_FAT=>draw_fat(cx,sy,cw,sh),
        VIEW_NTFS=>draw_ntfs(cx,sy,cw,sh),
        VIEW_TEXT=>draw_text(cx,sy,cw,sh),
        _=>{}
    }
    if unsafe{CTX}{draw_context();}
    if unsafe{CONFIRM_DEL}{draw_confirm(wx,wy,ww,wh);}
    if unsafe{PROPS_OPEN}{draw_props_dialog(wx,wy,ww,wh);}
    // Win7 details pane
    let status_y=wy+wh.saturating_sub(details_h+2);
    vgrad(wx+3,status_y,ww.saturating_sub(6),details_h,0x00F8FAFC,0x00E0E8F0);
    graphics::border_rect(wx+3,status_y,ww.saturating_sub(6),details_h,0x00A0A8B8);
    graphics::fill_rect(wx+3,status_y,1,details_h,0x00C0C8D0);
    unsafe{
        fill_status_bar();
        icon::blit(icon::IconId::Folder,wx+8,status_y+2,false);
        let mut i=0;while i<STATUS_LEN&&i<((ww.saturating_sub(48))/8){
            graphics::draw_char(wx+42+i*8,status_y+10,STATUS[i],TEXT);i+=1;
        }
    }
}


fn open_selected(){
    unsafe{
        if SEL<0{return;}
        let mut a=[fs::ListItem{name:[0;24],name_len:0,size:0,is_dir:false};16];
        let n=fs::list_ex_path(core::str::from_utf8_unchecked(&CWD[..CWD_LEN]),&mut a);
        let s=SEL as usize;
        if s>=n{return;}
        if a[s].is_dir{
            let mut j=0;
            while j<a[s].name_len&&CWD_LEN<32{
                if CWD_LEN>1{CWD[CWD_LEN]=b'/';CWD_LEN+=1;}
                CWD[CWD_LEN]=a[s].name[j];CWD_LEN+=1;j+=1;
            }
            SEL=-1;VIEW=VIEW_ROOT;status(b"folder opened");
            return;
        }
        // Build full path CWD + "/" + name
        let mut full=[0u8;96];
        let mut fl=0usize;
        let mut i=0;
        while i<CWD_LEN && fl<95 { full[fl]=CWD[i]; fl+=1; i+=1; }
        if fl==0 || full[fl-1]!=b'/' {
            if fl<95 { full[fl]=b'/'; fl+=1; }
        }
        i=0;
        while i<a[s].name_len && fl<95 { full[fl]=a[s].name[i]; fl+=1; i+=1; }
        let path=core::str::from_utf8_unchecked(&full[..fl]);
        if crate::media_player::is_audio_path(path) {
            if crate::media_player::open_from_explorer(path) {
                crate::media_player::play();
                crate::desktop::clear_media_pick();
                crate::desktop::open_media_player();
                status(b"Playing in Media Player");
                serial::write_str("[FILES] audio -> media player: ");
                serial::write_str(path);
                serial::write_str("\n");
            } else {
                status(b"Cannot play this audio file");
            }
            return;
        }
        if crate::video_player::is_video_path(path) {
            if crate::video_player::open_from_aetherfs(path) {
                crate::video_player::play();
                crate::desktop::open_video_player();
                status(b"Playing disk video (AetherFS)");
            } else {
                status(b"Cannot open video");
            }
            return;
        }
        // Native Aether ELF (e.g. /bin/sh, /bin/init) - Ring3 process
        if crate::userspace::path_is_elf(path) {
            status(b"Starting native ELF...");
            serial::write_str("[FILES] native ELF launch: ");
            serial::write_str(path);
            serial::write_str("\n");
            // Does not return on success
            let _ = crate::userspace::run_elf_path(path);
            status(b"ELF launch failed");
            return;
        }
        // Image files: try JPEG decode summary; still show text preview fallback
        let mut is_img = false;
        let mut is_jpg = false;
        {
            let nl = a[s].name_len;
            if nl >= 4 {
                let e0 = a[s].name[nl-3];
                let e1 = a[s].name[nl-2];
                let e2 = a[s].name[nl-1];
                // JPG/JPEG/BMP/PNG/GIF rough
                if (e0==b'J'||e0==b'j') && (e1==b'P'||e1==b'p') { is_img = true; is_jpg = true; }
                if (e0==b'B'||e0==b'b') && (e1==b'M'||e1==b'm') && (e2==b'P'||e2==b'p') { is_img = true; }
                if (e0==b'P'||e0==b'p') && (e1==b'N'||e1==b'n') && (e2==b'G'||e2==b'g') { is_img = true; }
                if (e0==b'G'||e0==b'g') && (e1==b'I'||e1==b'i') && (e2==b'F'||e2==b'f') { is_img = true; }
            }
        }
        let mut buf=[0u8;4096];
        if let Some(r)=fs::read_name(&a[s].name,a[s].name_len,&mut buf){
            if is_jpg && r > 64 {
                static mut RGB: [u32; 160*120] = [0; 160*120];
                let dim = unsafe { crate::jpeg_dec::decode_rgb(&buf[..r], &mut RGB, 160, 120) };
                if let Some((w,h)) = dim {
                    let mut msg=[0u8;64];
                    let mut p=0usize;
                    put_str(&mut msg,&mut p,b"JPEG OK ");
                    put_num(&mut msg,&mut p,w);
                    put_str(&mut msg,&mut p,b"x");
                    put_num(&mut msg,&mut p,h);
                    // Keep text preview of header
                    let mut i=0;while i<r&&i<512{PREVIEW[i]=buf[i];i+=1;}
                    PREVIEW_LEN=if r<512{r}else{512};
                    PREVIEW_FROM=VIEW_ROOT; VIEW=VIEW_TEXT;
                    status(&msg[..p]);
                    return;
                }
            }
            let mut i=0;while i<r&&i<2048{PREVIEW[i]=buf[i];i+=1;}
            PREVIEW_LEN=if r<2048{r}else{2048}; PREVIEW_FROM=VIEW_ROOT; VIEW=VIEW_TEXT;
            if is_img { status(b"Image opened as data preview"); }
            else { status(b"Preview opened"); }
        } else {
            status(b"Cannot open file");
        }
    }
}
fn selected_list_item(out:&mut fs::ListItem)->bool{
    unsafe{
        if SEL<0||VIEW!=VIEW_ROOT{return false;}
        let mut a=[fs::ListItem{name:[0;24],name_len:0,size:0,is_dir:false};16];
        let n=fs::list_ex_path(core::str::from_utf8_unchecked(&CWD[..CWD_LEN]),&mut a);
        let s=SEL as usize;
        if s>=n{return false;}
        *out=a[s];
        true
    }
}
fn build_full_path(name:&[u8],name_len:usize,out:&mut [u8;96])->usize{
    unsafe{
        let mut fl=0usize;
        let mut i=0;
        while i<CWD_LEN&&fl<95{out[fl]=CWD[i];fl+=1;i+=1;}
        if fl==0||out[fl-1]!=b'/'{if fl<95{out[fl]=b'/';fl+=1;}}
        i=0;
        while i<name_len&&fl<95{out[fl]=name[i];fl+=1;i+=1;}
        fl
    }
}
fn delete_selected(){
    unsafe{
        if !fs_writable()||SEL<0{status(b"Delete: read-only or no selection");return;}
        if VIEW==VIEW_FAT{
            if fs_fat::delete_entry(SEL as usize){
                SEL=-1;status(b"FAT item deleted");
            }else{status(b"FAT delete failed");}
            return;
        }
        let mut it=fs::ListItem{name:[0;24],name_len:0,size:0,is_dir:false};
        if !selected_list_item(&mut it){status(b"Delete failed");return;}
        let mut full=[0u8;96];
        let fl=build_full_path(&it.name,it.name_len,&mut full);
        let path=core::str::from_utf8_unchecked(&full[..fl]);
        if fs::delete(path)||fs::delete_name(&it.name,it.name_len){
            SEL=-1;status(b"Item deleted");
        }else{status(b"Delete failed");}
    }
}
fn new_folder(){
    unsafe{
        if !fs_writable(){status(b"New folder: AetherFS or FAT only");return;}
        if VIEW==VIEW_ROOT{
            if fs::mkdir("/New Folder"){status(b"New folder created");}
            else{status(b"Cannot create folder");}
        }else if VIEW==VIEW_FAT{
            if fs_fat::mkdir(b"NEWFOLDER"){
                status(b"FAT: folder NEWFOLDER created");
            }else if fs_fat::mkdir(b"NEWDIR"){
                status(b"FAT: folder NEWDIR created");
            }else{status(b"FAT mkdir failed");}
        }
    }
}
fn new_file(){
    unsafe{
        if !fs_writable(){status(b"New file: AetherFS or FAT only");return;}
        if VIEW==VIEW_ROOT{
            if fs::create("/New Text Document.txt"){
                let _=fs::write("/New Text Document.txt",b"New document\n");
                status(b"Text document created");
            }else{status(b"Cannot create file");}
        }else if VIEW==VIEW_FAT{
            if fs_fat::write_file(b"NEW.TXT",b"New document\n"){
                status(b"FAT: NEW.TXT created");
            }else{status(b"FAT create failed");}
        }
    }
}


fn rename_selected(){
    unsafe{
        if !fs_writable()||SEL<0{status(b"Rename: read-only or no selection");return;}
        // Simple sequential rename: append _2 / use fixed pattern then user can cut
        if VIEW==VIEW_FAT{
            let mut name=[0u8;13];
            if let Some(e)=fs_fat::entry(SEL as usize){
                let mut n=0usize;
                while n<e.name_len&&n<8{name[n]=e.name[n];n+=1;}
                name[n]=b'_';n+=1;
                name[n]=b'2';n+=1;
                // preserve extension
                let mut i=0usize;
                while i<e.name_len{
                    if e.name[i]==b'.'{
                        while i<e.name_len&&n<12{name[n]=e.name[i];n+=1;i+=1;}
                        break;
                    }
                    i+=1;
                }
                if fs_fat::rename_entry(SEL as usize,&name[..n]){
                    status(b"FAT renamed (+_2)");
                }else{status(b"FAT rename failed");}
            }
            return;
        }
        let mut it=fs::ListItem{name:[0;24],name_len:0,size:0,is_dir:false};
        if !selected_list_item(&mut it){status(b"Rename failed");return;}
        let mut full=[0u8;96];
        let fl=build_full_path(&it.name,it.name_len,&mut full);
        let old=core::str::from_utf8_unchecked(&full[..fl]);
        if fs::rename(old,"Renamed_Item")||fs::rename(old,"Renamed_Item_2"){
            status(b"Renamed");
        }else{status(b"Rename failed");}
    }
}


fn clipboard_clear(){
    unsafe{
        CLIP_MODE=0;CLIP_IS_DIR=false;CLIP_NAME_LEN=0;CLIP_PATH_LEN=0;CLIP_DATA_LEN=0;
        let mut i=0;while i<48{CLIP_NAME[i]=0;i+=1;}
        i=0;while i<96{CLIP_PATH[i]=0;i+=1;}
    }
}
fn clipboard_set_from_selection(cut:bool){
    unsafe{
        if SEL<0{status(b"Clipboard: select a file first");return;}
        if cut&&!fs_writable(){
            status(b"Cut: NTFS is read-only (use FAT or AetherFS)");
            return;
        }
        clipboard_clear();
        CLIP_MODE=if cut{2}else{1};
        CLIP_DATA_LEN=0;
        CLIP_IS_DIR=false;
        CLIP_SRC=0;

        if VIEW==VIEW_ROOT{
            let mut it=fs::ListItem{name:[0;24],name_len:0,size:0,is_dir:false};
            if !selected_list_item(&mut it){status(b"Clipboard: no item");CLIP_MODE=0;return;}
            CLIP_IS_DIR=it.is_dir;
            CLIP_NAME_LEN=it.name_len.min(48);
            let mut i=0;while i<CLIP_NAME_LEN{CLIP_NAME[i]=it.name[i];i+=1;}
            CLIP_PATH_LEN=build_full_path(&it.name,it.name_len,&mut CLIP_PATH);
            CLIP_SRC=1; // AetherFS
            if !it.is_dir{
                let path=core::str::from_utf8_unchecked(&CLIP_PATH[..CLIP_PATH_LEN]);
                if let Some(n)=fs::read_large(path,&mut CLIP_DATA){
                    CLIP_DATA_LEN=n.min(65536);
                }else if let Some(n)=fs::read(path,&mut CLIP_DATA){
                    CLIP_DATA_LEN=n.min(65536);
                }else if let Some(n)=fs::read_name(&it.name,it.name_len,&mut CLIP_DATA){
                    CLIP_DATA_LEN=n.min(65536);
                }
            }
        }else if VIEW==VIEW_NTFS{
            if NTFS_VIEW_DIRTY{rebuild_ntfs_view();}
            let abs=(NTFS_SCROLL as i32+SEL) as usize;
            if abs>=NTFS_VIEW_COUNT{status(b"Clipboard: bad NTFS row");CLIP_MODE=0;return;}
            let idx=NTFS_VIEW_IDX[abs] as usize;
            if let Some(e)=fs_ntfs::entry(idx){
                CLIP_IS_DIR=e.is_dir;
                CLIP_NAME_LEN=e.name_len.min(48);
                let mut i=0;while i<CLIP_NAME_LEN{CLIP_NAME[i]=e.name[i];i+=1;}
                CLIP_PATH_LEN=0;
                CLIP_MFT=e.mft_ref;
                CLIP_SRC=2;
                if e.is_dir{
                    status(b"Copy folder: not supported yet");
                    CLIP_MODE=0;return;
                }
                match fs_ntfs::read_file_prefix(e.mft_ref,&mut CLIP_DATA){
                    Some(n)=>{CLIP_DATA_LEN=n.min(65536);}
                    None=>{status(b"Copy: NTFS read failed");CLIP_MODE=0;return;}
                }
            }else{status(b"Clipboard: NTFS entry missing");CLIP_MODE=0;return;}
        }else if VIEW==VIEW_FAT{
            let i=SEL as usize;
            if let Some(e)=fs_fat::entry(i){
                CLIP_IS_DIR=e.is_dir;
                CLIP_NAME_LEN=e.name_len.min(48);
                let mut k=0;while k<CLIP_NAME_LEN{CLIP_NAME[k]=e.name[k];k+=1;}
                CLIP_PATH_LEN=0;
                CLIP_FAT_IDX=i as u32;
                CLIP_SRC=3;
                if e.is_dir{
                    status(b"Copy folder: not supported yet");
                    CLIP_MODE=0;return;
                }
                match fs_fat::read_file(i,&mut CLIP_DATA){
                    Some(n)=>{CLIP_DATA_LEN=n.min(65536);}
                    None=>{status(b"Copy: FAT read failed");CLIP_MODE=0;return;}
                }
            }else{status(b"Clipboard: FAT entry missing");CLIP_MODE=0;return;}
        }else{
            status(b"Clipboard: open a folder first");
            CLIP_MODE=0;return;
        }

        if cut{status(b"Cut to clipboard");}
        else if CLIP_DATA_LEN>0{
            let mut b=[0u8;64];let mut p=0usize;
            put_str(&mut b,&mut p,b"Copied ");
            put_num(&mut b,&mut p,CLIP_DATA_LEN);
            put_str(&mut b,&mut p,b" bytes");
            status(&b[..p]);
        }else{status(b"Copied (empty/dir)");}
    }
}
/// Build unique paste name: name.ext -> name_2.ext, name_3.ext ...
fn unique_paste_name(out: &mut [u8; 64]) -> usize {
    unsafe {
        let mut base = [0u8; 40];
        let mut ext = [0u8; 8];
        let mut bl = 0usize;
        let mut el = 0usize;
        let mut dot = CLIP_NAME_LEN;
        let mut i = 0usize;
        while i < CLIP_NAME_LEN {
            if CLIP_NAME[i] == b'.' {
                dot = i;
            }
            i += 1;
        }
        if dot < CLIP_NAME_LEN && dot > 0 {
            while bl < dot && bl < 32 {
                base[bl] = CLIP_NAME[bl];
                bl += 1;
            }
            let mut j = dot;
            while j < CLIP_NAME_LEN && el < 7 {
                ext[el] = CLIP_NAME[j];
                el += 1;
                j += 1;
            }
        } else {
            while bl < CLIP_NAME_LEN && bl < 32 {
                base[bl] = CLIP_NAME[bl];
                bl += 1;
            }
        }
        // try original, then _2 .. _9
        let mut attempt = 0u8;
        while attempt < 10 {
            let mut n = 0usize;
            let mut k = 0usize;
            while k < bl && n < 40 {
                out[n] = base[k];
                n += 1;
                k += 1;
            }
            if attempt > 0 {
                if n < 46 {
                    out[n] = b'_';
                    n += 1;
                }
                if n < 47 {
                    out[n] = b'0' + attempt;
                    n += 1;
                }
            }
            k = 0;
            while k < el && n < 47 {
                out[n] = ext[k];
                n += 1;
                k += 1;
            }
            // exists?
            let mut exists = false;
            if VIEW == VIEW_FAT && fs_fat::is_mounted() {
                let cnt = fs_fat::entry_count();
                let mut e = 0usize;
                while e < cnt {
                    if let Some(ent) = fs_fat::entry(e) {
                        if ent.name_len == n {
                            let mut same = true;
                            let mut c = 0usize;
                            while c < n {
                                let a = ent.name[c];
                                let b = out[c];
                                let al = if a >= b'A' && a <= b'Z' { a + 32 } else { a };
                                let blc = if b >= b'A' && b <= b'Z' { b + 32 } else { b };
                                if al != blc {
                                    same = false;
                                    break;
                                }
                                c += 1;
                            }
                            if same {
                                exists = true;
                                break;
                            }
                        }
                    }
                    e += 1;
                }
            } else if VIEW == VIEW_ROOT && fs::is_mounted() {
                let mut path = [0u8; 64];
                path[0] = b'/';
                let mut p = 1usize;
                let mut c = 0usize;
                while c < n && p < 63 {
                    path[p] = out[c];
                    p += 1;
                    c += 1;
                }
                let mut buf = [0u8; 4];
                if fs::read(core::str::from_utf8_unchecked(&path[..p]), &mut buf).is_some() {
                    exists = true;
                }
            }
            if !exists {
                return n;
            }
            attempt += 1;
        }
        // fallback
        let fb = b"PASTE.BIN";
        let mut n = 0usize;
        while n < fb.len() {
            out[n] = fb[n];
            n += 1;
        }
        n
    }
}

fn clipboard_paste(){
    unsafe{
        if CLIP_MODE==0{status(b"Clipboard empty");return;}
        if !fs_writable(){status(b"Paste: need AetherFS or FAT folder");return;}
        if CLIP_IS_DIR{status(b"Paste folder: not supported yet");return;}
        let mut name=[0u8;64];
        let nlen=unique_paste_name(&mut name);
        let mut ok=false;
        if VIEW==VIEW_ROOT&&crate::fs::is_mounted(){
            let mut dest=[0u8;64];
            dest[0]=b'/';
            let mut dlen=1usize;
            let mut i=0;
            while i<nlen&&dlen<63{dest[dlen]=name[i];dlen+=1;i+=1;}
            let dest_s=core::str::from_utf8_unchecked(&dest[..dlen]);
            if fs::create(dest_s){
                ok=fs::write(dest_s,&CLIP_DATA[..CLIP_DATA_LEN])||fs::write_large(dest_s,&CLIP_DATA[..CLIP_DATA_LEN]);
            }
        }else if VIEW==VIEW_FAT&&crate::fs_fat::is_mounted(){
            ok=fs_fat::write_file(&name[..nlen],&CLIP_DATA[..CLIP_DATA_LEN]);
        }
        if !ok{status(b"Paste failed");return;}
        if CLIP_MODE==2{
            if CLIP_SRC==1{
                let src=core::str::from_utf8_unchecked(&CLIP_PATH[..CLIP_PATH_LEN]);
                let _=fs::delete(src);
                let _=fs::delete_name(&CLIP_NAME,CLIP_NAME_LEN);
            }else if CLIP_SRC==3{
                let _=fs_fat::delete_entry(CLIP_FAT_IDX as usize);
            }
            clipboard_clear();
            status(b"Moved");
        }else{
            let mut b=[0u8;64];
            let mut p=0usize;
            put_str(&mut b,&mut p,b"Pasted ");
            let mut i=0usize;
            while i<nlen&&p<47{b[p]=name[i];p+=1;i+=1;}
            status(&b[..p]);
        }
        SEL=-1;
        if VIEW==VIEW_FAT{let _=fs_fat::list_cwd();}
    }
}

pub fn on_click(wx:i32,wy:i32,ww:i32,wh:i32,title_h:i32,mx:i32,my:i32,right:bool,double_click:bool)->bool{
    let by=wy+title_h;let relx=mx-wx;let rely=my-by;
    unsafe{
        // Default full redraw; selection-only paths call hint_sel() and override.
        PAINT_HINT=2;
        if PROPS_OPEN{
            let px=wx+ww/2-160;let py=wy+wh/2-130;
            if (mx>=px+272&&mx<px+308&&my>=py+4&&my<py+28) || (right && mx>=px&&mx<px+320&&my>=py&&my<py+260) {
                PROPS_OPEN=false;
            }
            return true;
        }
        if CONFIRM_DEL{let cx=ww/2;let cy=wh/2;if my>wy+cy+20&&my<wy+cy+60{if mx<wx+cx{delete_selected();}CONFIRM_DEL=false;}return true;}
        if CTX{
            let ctx_h=4+15*CTX_ROW+8;
            if mx>=CTX_X&&mx<CTX_X+CTX_W&&my>=CTX_Y&&my<CTX_Y+ctx_h{
                let rel=my-CTX_Y;
                let r=match context_item(rel){Some(v)=>v,None=>{CTX=false;return true;}};
                match r{
                    0=>open_selected(),
                    1=>{
                        // Open with: Media Player / Video Player / text preview
                        if SEL>=0 && VIEW==VIEW_ROOT {
                            let mut a=[fs::ListItem{name:[0;24],name_len:0,size:0,is_dir:false};16];
                            let n=fs::list_ex_path(core::str::from_utf8_unchecked(&CWD[..CWD_LEN]),&mut a);
                            let s=SEL as usize;
                            if s<n && !a[s].is_dir {
                                let mut full=[0u8;96]; let fl=build_full_path(&a[s].name,a[s].name_len,&mut full);
                                let path=core::str::from_utf8_unchecked(&full[..fl]);
                                if crate::media_player::is_audio_path(path) {
                                    if crate::media_player::open_from_explorer(path) {
                                        crate::media_player::play();
                                        crate::desktop::open_media_player();
                                        status(b"Opened in Media Player");
                                    } else { status(b"Media Player cannot open file"); }
                                } else if crate::video_player::is_video_path(path) {
                                    if crate::video_player::open_from_aetherfs(path) {
                                        crate::video_player::play();
                                        crate::desktop::open_video_player();
                                        status(b"Opened in Video Player");
                                    } else { status(b"Video Player cannot open file"); }
                                } else {
                                    open_selected();
                                    status(b"Opened with text preview");
                                }
                            }
                        } else if VIEW==VIEW_FAT {
                            if SEL>=0 {
                                if let Some(e)=fs_fat::entry(SEL as usize) {
                                    if e.is_dir {
                                        let _=fs_fat::enter_dir(SEL as usize);
                                        SEL=-1; status(b"Opened folder");
                                    } else {
                                        open_selected();
                                        status(b"Opened with default handler");
                                    }
                                }
                            }
                        } else if VIEW==VIEW_NTFS {
                            status(b"Open with: use Open on NTFS");
                        } else { status(b"Open with: select a file"); }
                    },
                    2=>clipboard_set_from_selection(false),
                    3=>clipboard_set_from_selection(true),
                    4=>clipboard_paste(),
                    5=>rename_selected(),
                    6=>{CONFIRM_DEL=true;status(b"Confirm delete");},
                    7=>{fill_props_selected();},
                    8=>{
                        if VIEW==VIEW_NTFS{
                            if fs_ntfs::list_directory(NTFS_CWD_REF){NTFS_VIEW_DIRTY=true;NTFS_SCROLL=0;trace_ntfs(b"DIR");}
                        }else if VIEW==VIEW_FAT{
                            let _=fs_fat::list_cwd();
                        }else{
                            let _=fs::list_ex_path(core::str::from_utf8_unchecked(&CWD[..CWD_LEN]),&mut [fs::ListItem{name:[0;24],name_len:0,size:0,is_dir:false};16]);
                        }
                        SEL=-1;status(b"Refreshed");
                    },
                    9=>new_folder(),
                    10=>new_file(),
                    _=>{}
                }
                CTX=false;
                return true;
            }
            CTX=false;
            return true;
        }
        if right{
            status(b"RCLICK");
            let side=150i32.min(ww/3);
            let list_top=by+56;
            if mx>=wx+side+8&&my>=list_top{
                if VIEW==VIEW_NTFS{
                    if unsafe{NTFS_VIEW_DIRTY}{rebuild_ntfs_view();}
                    let vis=unsafe{ if NTFS_VISIBLE_ROWS==0 {1} else {NTFS_VISIBLE_ROWS} };
                    if my>=list_top+ROW_TOP{
                        let row=hit_row(list_top,my);
                        // visible-row only (not total entry count)
                        if row>=0 && (row as usize)<vis { SEL=row; }
                    }
                }else if VIEW==VIEW_ROOT{
                    let mut a=[fs::ListItem{name:[0;24],name_len:0,size:0,is_dir:false};16];
                    let n=fs::list_ex_path(core::str::from_utf8_unchecked(&CWD[..CWD_LEN]),&mut a);
                    if my>=list_top+ROW_TOP{
                        let row=hit_row(list_top,my);
                        if row>=0&&row<n as i32{SEL=row;}
                    }
                }
            }
            CTX=true;
            // Keep popup fully on-screen
            let sw=graphics::width() as i32;
            let sh=graphics::height() as i32;
            let mh=4+15*CTX_ROW+8;
            let mut cx=mx;
            let mut cy=my;
            if cx+CTX_W>sw-4{cx=sw-CTX_W-4;}
            if cy+mh>sh-4{cy=sh-mh-4;}
            if cx<4{cx=4;}
            if cy<4{cy=4;}
            CTX_X=cx;
            CTX_Y=cy;
            status(b"Context menu");
            log(b"context menu opened");
            return true;
        }
        // Nav row (Back/Fwd/Up)
        if rely>=5&&rely<34{
            if relx<44{go_back();}
            else if relx<80{go_forward();}
            else if relx<116{go_up();}
            else if relx>=112&&relx<112+200{
                // click address bar -> Computer
                VIEW=VIEW_COMPUTER; SEL=-1; COMPUTER_SEL=-1;
                status(b"This PC");
            }
            return true;
        }
        // Command bar
        if rely>=34&&rely<55{
            if relx<80{
                CTX=true; CTX_X=wx+12; CTX_Y=by+56;
                status(b"Organize menu");
            } else if relx<170{
                new_folder();
            } else if relx<250{
                new_file();
            } else if relx<310{
                clipboard_paste();
            } else if relx<390{
                // Refresh
                if VIEW==VIEW_FAT { let _=fs_fat::list_cwd(); }
                else if VIEW==VIEW_NTFS {
                    if NTFS_CWD_REF!=0 && NTFS_CWD_REF!=0xFFFF_FFFF {
                        if fs_ntfs::list_directory(NTFS_CWD_REF){NTFS_VIEW_DIRTY=true;NTFS_SCROLL=0;}
                    }
                }
                status(b"Refreshed");
            }
            return true;
        }
        let side=160i32.min(ww/3);let list_top=by+56;
        // Left sidebar navigation (Win7-style)
        if mx>=wx+3&&mx<wx+side&&my>=list_top&&my<list_top+wh.saturating_sub(title_h+78){
            // Approximate row positions matching draw_sidebar
            let rel=my-list_top;
            // Favorites/Desktop ~10+18=28; Downloads +28; Recent +28; Libraries +32...
            // Computer section ~ row after Pictures
            // y offsets from draw_sidebar: start 10, +18 header, +28*3 fav, +32 lib header area...
            // Simpler hit zones:
            if rel>=200&&rel<230{
                // Local Disk (C:) under Computer
                VIEW=VIEW_DISK;SEL=-1;status(b"Local Disk");
                return true;
            }
            if rel>=230&&rel<260{
                // AetherFS
                CWD[0]=b'/';CWD_LEN=1;VIEW=VIEW_ROOT;SEL=-1;status(b"AetherFS");
                return true;
            }
            if rel>=160&&rel<200{
                // Computer header / This PC
                VIEW=VIEW_COMPUTER;SEL=-1;COMPUTER_SEL=-1;status(b"Computer");
                return true;
            }
            if rel>=28&&rel<56{
                // Desktop favorite -> AetherFS root
                CWD[0]=b'/';CWD_LEN=1;VIEW=VIEW_ROOT;SEL=-1;status(b"Desktop / AetherFS");
                return true;
            }
            return true; // absorb sidebar click
        }
        if mx>=wx+side+8&&my>=list_top{
            if VIEW==VIEW_COMPUTER{
                let pn=part::count();
                let mut disks=[false;8];
                let mut d=0usize;
                while d<8{
                    let mut j=0usize;
                    while j<pn{
                        if let Some(p)=part::get(j){if p.disk as usize==d{disks[d]=true;break;}}
                        j+=1;
                    }
                    d+=1;
                }
                let mut card=0usize;
                let mut dd=0usize;
                while dd<8{
                    if disks[dd]{
                        let top=list_top+45+(card*82) as i32;
                        if my>=top&&my<top+74{
                            if double_click {
                                let mut j=0usize;
                                while j<pn{
                                    if let Some(p)=part::get(j){
                                        if p.disk as usize==dd{
                                            SEL=j as i32; VIEW=VIEW_DISK;
                                            COMPUTER_SEL=dd as i32;
                                            status(b"Physical disk opened");
                                            return true;
                                        }
                                    }
                                    j+=1;
                                }
                                // disk with no partitions still enter disk view
                                VIEW=VIEW_DISK; SEL=-1; COMPUTER_SEL=dd as i32;
                                status(b"Disk (no partitions listed)");
                                return true;
                            }
                            COMPUTER_SEL=dd as i32;
                            SEL=-1;
                            status(b"Selected");
                            return true;
                        }
                        card+=1;
                    }
                    dd+=1;
                }
                let atop=list_top+45+(card*82) as i32;
                if my>=atop&&my<atop+74{
                    if double_click {
                        push_hist(); set_root(); status(b"AetherFS (A:) opened"); return true;
                    }
                    COMPUTER_SEL=8;
                    SEL=-1;
                    status(b"Selected");
                    return true;
                }
                // USB Phone card (row after AetherFS)
                let ptop=atop+82;
                if my>=ptop&&my<ptop+74{
                    if double_click {
                        if crate::drivers::usb_phone::is_mounted() {
                            status(b"USB Phone (P:) mounted via MSC");
                        } else if crate::drivers::usb_phone::is_connected() {
                            status(b"Phone detected - run PH to mount");
                        } else {
                            status(b"No phone - plug USB, File transfer, PH");
                        }
                        return true;
                    }
                    COMPUTER_SEL=9;
                    SEL=-1;
                    status(b"USB Phone selected");
                    return true;
                }
            }
            else if VIEW==VIEW_DISK{
                let n=part::count();
                if my<list_top+52{return false;}
                let row=((my-list_top-52)/ROW_H)as i32;
                if row>=0&&row<n as i32{
                    SEL=row;
                    if double_click {
                        if let Some(p)=part::get(row as usize){
                            crate::drivers::audio::playback_stop();
                            trace_partition_click(mx,my,row,p.index as i32);
                            log(b"partition open requested");
                            if p.ptype==0x07{
                                if fs_ntfs::mount_partition(row as usize){
                                    VIEW=VIEW_NTFS;SEL=-1;NTFS_CWD_REF=5;NTFS_DEPTH=0;NTFS_VIEW_DIRTY=true;NTFS_SCROLL=0;
                                    trace_ntfs(b"OPEN");
                                }else{status(b"NTFS mount failed - read-only");}
                            }else if p.ptype==0x0B||p.ptype==0x0C||p.ptype==0x06||p.ptype==0x0E||p.ptype==0x04{
                                if fs_fat::mount_partition(row as usize){VIEW=VIEW_FAT;SEL=-1;status(b"FAT root opened read-only");}
                                else{status(b"FAT mount failed - read-only");}
                            }else{status(b"ext4: not in this build (NTFS/FAT/AetherFS only)");}
                        }
                        return true;
                    }
                    status(b"Selected");
                    if let Some(p)=part::get(row as usize){
                        trace_partition_click(mx,my,row,p.index as i32);
                        return true;
                    }
                    return true;
                }
                return false;
            }
            else if VIEW==VIEW_NTFS{
                if unsafe{NTFS_VIEW_DIRTY}{rebuild_ntfs_view();}
                let n=unsafe{NTFS_VIEW_COUNT};
                // Column header sort
                if my>=list_top && my<list_top+ROW_TOP{
                    let side=150i32.min(ww/3);
                    let lx=mx-(wx+side+8);
                    if lx<244{ SORT_MODE=0; }
                    else if lx<384{ SORT_MODE=2; }
                    else if lx<454{ /* type - keep name */ SORT_MODE=0; }
                    else { SORT_MODE=1; }
                    NTFS_VIEW_DIRTY=true;
                    let msg=if SORT_MODE==0{b"Sort: Name"}else if SORT_MODE==1{b"Sort: Size"}else{b"Sort: Date"};
                    status(msg);
                    return true;
                }
                // Scrollbar track = right 14px of list body only (not status strip).
                // Recompute vis from window height so bar_h matches drawn track even
                // if NTFS_VISIBLE_ROWS is stale after resize.
                let side=150i32.min(ww/3);
                let list_left=wx+side+8;
                let list_w=(ww-side-16).max(1);
                let bar_x=list_left+list_w-14;
                // list body ends above status strip (18px) + small pad
                let list_bottom = wy + wh - 20;
                let list_h = (list_bottom - list_top).max(1) as usize;
                let vis_calc = visible_rows(list_h);
                let vis = if vis_calc == 0 { 1 } else { vis_calc };
                NTFS_VISIBLE_ROWS = vis;
                let bar_top = list_top + ROW_TOP;
                let bar_h_i = (vis as i32).saturating_mul(ROW_H);
                let bar_bot = bar_top + bar_h_i;
                // Entire right gutter of list body: always consume (prevents
                // fall-through into row open / window-edge resize races).
                if mx >= bar_x && mx < list_left + list_w && my >= bar_top && my < list_bottom {
                    if n > vis && bar_h_i > 0 {
                        let rel = my - bar_top;
                        // Clamp into [0, bar_h_i-1] even if click is past drawn thumb track
                        let rel_c = if rel <= 0 {
                            0usize
                        } else if rel >= bar_h_i {
                            (bar_h_i as usize).saturating_sub(1)
                        } else {
                            rel as usize
                        };
                        let max_s = n.saturating_sub(vis);
                        let den = bar_h_i as usize;
                        let mut sc = if den == 0 { 0 } else { rel_c.saturating_mul(max_s) / den };
                        if sc > max_s { sc = max_s; }
                        NTFS_SCROLL = sc;
                        if SEL >= 0 && (SEL as usize) >= vis { SEL = (vis as i32) - 1; }
                        status(b"Scrolled");
                    }
                    let _ = bar_bot; // silence if unused in some builds
                    return true;
                }
                if my<list_top+ROW_TOP{return false;}
                let row=hit_row(list_top,my);
                // row is a *visible* index - never compare against total n
                let vis_i=vis as i32;
                if row>=0 && row<vis_i {
                    let old_sel=SEL;
                    SEL=row;
                    let idx=match ntfs_row_index((unsafe{NTFS_SCROLL})+(row as usize)){Some(v)=>v,None=>return false};
                    // Open: double-click OR second click on already selected row (reliable on AH532)
                    let activate = double_click || (old_sel == row && old_sel >= 0);
                    if activate {
                    hint_full();
                    if let Some(e)=fs_ntfs::entry(idx){
                        if e.is_dir{
                            if fs_ntfs::list_directory(e.mft_ref){
                                NTFS_VIEW_DIRTY=true;NTFS_SCROLL=0;
                                if NTFS_DEPTH<8{
                                    NTFS_PARENT[NTFS_DEPTH]=NTFS_CWD_REF;
                                    // remember folder name for address bar
                                    let mut k=0usize;
                                    while k<28{NTFS_SEG[NTFS_DEPTH][k]=0;k+=1;}
                                    k=0;
                                    while k<e.name_len && k<27{
                                        NTFS_SEG[NTFS_DEPTH][k]=e.name[k]; k+=1;
                                    }
                                    NTFS_SEG_LEN[NTFS_DEPTH]=k;
                                    NTFS_DEPTH+=1;
                                }
                                NTFS_CWD_REF=e.mft_ref;
                                SEL=-1;
                                status(b"Folder opened");
                                trace_ntfs(b"DIR");
                            }else{
                                status(b"NTFS folder read failed");
                            }
                        }else{
                            // Open audio in Media Player when extension matches.
                            let mut path=[0u8;64];
                            let mut pl=0usize;
                            let mut j=0usize;
                            while j<e.name_len && pl+1<64 {
                                path[pl]=e.name[j];
                                pl+=1; j+=1;
                            }
                            let pname=core::str::from_utf8_unchecked(&path[..pl]);
                            if crate::media_player::is_audio_path(pname) {
                                // Stream real file from NTFS MFT - never substitute embedded Mozart.
                                if crate::media_player::open_from_ntfs(e.mft_ref, &e.name[..e.name_len]) {
                                    crate::media_player::play();
                                    crate::desktop::clear_media_pick();
                                    crate::desktop::open_media_player();
                                    status(b"Playing disk track");
                                } else {
                                    status(b"Cannot open disk audio");
                                }
                            } else if crate::video_player::is_video_path(pname) {
                                if crate::video_player::open_from_ntfs(e.mft_ref, &e.name[..e.name_len]) {
                                    crate::video_player::play();
                                    crate::desktop::open_video_player();
                                    status(b"Playing disk video");
                                } else {
                                    status(b"Cannot open disk video");
                                }
                            } else if crate::pe::is_exe_name(&e.name[..e.name_len]) {
                                // Windows PE from disk - probe only; full Win32 not ready.
                                let msg = crate::pe::try_launch_ntfs(e.mft_ref, &e.name[..e.name_len]);
                                // Build preview report for user
                                let mut buf=[0u8;2048];
                                let mut n=0usize;
                                let push=|buf:&mut [u8],n:&mut usize,s:&[u8]|{
                                    let mut i=0; while i<s.len() && *n<buf.len(){buf[*n]=s[i];*n+=1;i+=1;}
                                };
                                push(&mut buf,&mut n,b"=== WINDOWS PROGRAM ===
");
                                push(&mut buf,&mut n,b"File: ");
                                let mut i=0; while i<e.name_len && n<buf.len(){buf[n]=e.name[i];n+=1;i+=1;}
                                push(&mut buf,&mut n,b"
");
                                if let Some(info)=crate::pe::probe_ntfs(e.mft_ref) {
                                    push(&mut buf,&mut n,b"Format: ");
                                    push(&mut buf,&mut n, match info.kind {
                                        crate::pe::PeKind::Pe32 => b"PE32",
                                        crate::pe::PeKind::Pe32Plus => b"PE32+",
                                        _ => b"?",
                                    });
                                    push(&mut buf,&mut n,b"
CPU: ");
                                    push(&mut buf,&mut n, crate::pe::machine_name(info.machine).as_bytes());
                                    push(&mut buf,&mut n,b"
Subsystem: ");
                                    push(&mut buf,&mut n, crate::pe::subsystem_name(info.subsystem).as_bytes());
                                    push(&mut buf,&mut n,b"
Entry RVA: 0x");
                                    // hex entry
                                    let mut hx=[0u8;8]; let mut v=info.entry_rva; let mut hi=8usize;
                                    while hi>0{hi-=1; let d=(v&0xF) as u8; hx[hi]=if d<10{b'0'+d}else{b'A'+(d-10)}; v>>=4;}
                                    let mut hi=0; while hi<8 && n<buf.len(){buf[n]=hx[hi];n+=1;hi+=1;}
                                    push(&mut buf,&mut n,b"
Imports: ");
                                    push(&mut buf,&mut n, if info.has_imports {b"YES (needs Win32)"} else {b"none"});
                                    push(&mut buf,&mut n,b"
Image size: ");
                                    // decimal size
                                    let mut tmp=[0u8;12]; let mut tv=info.size_of_image; let mut tp=0usize;
                                    if tv==0 {tmp[0]=b'0'; tp=1;} else {
                                        while tv>0 && tp<12 { tmp[tp]=b'0'+((tv%10) as u8); tv/=10; tp+=1; }
                                        // reverse
                                        let mut a=0; let mut b=tp; while a<b { b-=1; let x=tmp[a]; tmp[a]=tmp[b]; tmp[b]=x; a+=1; }
                                    }
                                    let mut i=0; while i<tp && n<buf.len(){buf[n]=tmp[i];n+=1;i+=1;}
                                    push(&mut buf,&mut n,b" bytes
");
                                }
                                push(&mut buf,&mut n,b"
Status: ");
                                push(&mut buf,&mut n, msg.as_bytes());
                                push(&mut buf,&mut n,b"

Windows personality stage 1:
");
                                push(&mut buf,&mut n,b"PE sections loaded into process
");
                                push(&mut buf,&mut n,b"if status says LOADED.
");
                                push(&mut buf,&mut n,b"
Still required to RUN:
");
                                push(&mut buf,&mut n,b"- Import resolve (ntdll/kernel32)
");
                                push(&mut buf,&mut n,b"- PEB/TEB + NT syscall ABI
");
                                push(&mut buf,&mut n,b"- Then entry in Ring3
");
                                let mut i=0; while i<n && i<2048 { PREVIEW[i]=buf[i]; i+=1; }
                                PREVIEW_LEN=n.min(2048);
                                PREVIEW_FROM=VIEW_NTFS;
                                VIEW=VIEW_TEXT;
                                status(msg.as_bytes());
                            } else {
                                // Resident $DATA text preview when possible
                                let mut buf=[0u8;2048];
                                match fs_ntfs::read_file_prefix(e.mft_ref, &mut buf) {
                                    Some(n) => {
                                        if n==0 {
                                            status(b"Empty file");
                                        } else {
                                            let mut i=0usize;
                                            while i<n && i<2048 { PREVIEW[i]=buf[i]; i+=1; }
                                            PREVIEW_LEN=n;
                                            PREVIEW_FROM=VIEW_NTFS;
                                            VIEW=VIEW_TEXT;
                                            status(b"Preview opened");
                                        }
                                    }
                                    None => {
                                        let t=file_type_label(&e.name,e.name_len,false);
                                        if t=="Audio" { status(b"Audio: use Media Player path"); }
                                        else if t=="Text" { status(b"Cannot read file data"); }
                                        else { status(b"Cannot read file data"); }
                                    }
                                }
                            }
                        }
                    }
                    } else {
                        // Single click: select only - partial row paint (no full window flash)
                        hint_sel(old_sel);
                        fill_status_bar();
                    }
                    return true;
                }
                return false;
            }
            else if VIEW==VIEW_FAT{
                let n=fs_fat::entry_count();
                if my<list_top+ROW_TOP{return false;}
                let row=hit_row(list_top,my);
                if row>=0 && (row as usize)<n && row<32{
                    SEL=row;
                    if double_click{
                        if let Some(e)=fs_fat::entry(row as usize){
                            if e.is_dir{
                                if fs_fat::enter_dir(row as usize){
                                    SEL=-1; status(b"FAT folder opened");
                                }else{status(b"Cannot open FAT folder");}
                            }else{
                                PREVIEW_FROM=VIEW_FAT; status(b"FAT file selected");
                            }
                        }
                    }else{status(b"Selected");}
                    return true;
                }
                return false;
            }
            else if VIEW==VIEW_ROOT{
                let mut a=[fs::ListItem{name:[0;24],name_len:0,size:0,is_dir:false};16];
                let n=fs::list_ex_path(core::str::from_utf8_unchecked(&CWD[..CWD_LEN]),&mut a);
                if my<list_top+ROW_TOP{return false;}
                let row=hit_row(list_top,my);
                if row>=0&&row<n as i32{
                    SEL=row;
                    if double_click{
                        log(b"open selected");
                        open_selected();
                    }else{
                        log(b"selection changed");
                        status(b"Selected");
                    }
                    return true;
                }
                // Empty list area: no redraw.
                return false;
            }
            return true;
        }
        if my>=list_top&&my<list_top+330&&mx<wx+side{
            // Quick Access
            if my>=list_top+28&&my<list_top+56{
                push_hist(); set_root(); status(b"Desktop -> AetherFS"); return true;
            }
            if my>=list_top+60&&my<list_top+88{
                status(b"Documents: not available"); return true;
            }
            if my>=list_top+92&&my<list_top+120{
                status(b"Downloads: not available"); return true;
            }
            // This PC
            if my>=list_top+137&&my<list_top+165{
                VIEW=VIEW_COMPUTER; SEL=-1; COMPUTER_SEL=-1; status(b"This PC"); return true;
            }
            if my>=list_top+169&&my<list_top+197{
                if fs_ntfs::mount_first(){
                    trace_ntfs(b"SIDEBAR");
                    VIEW=VIEW_NTFS; SEL=-1; NTFS_CWD_REF=5; NTFS_DEPTH=0;
                    NTFS_VIEW_DIRTY=true; NTFS_SCROLL=0;
                    status(b"Local Disk (C:) opened");
                }else{ status(b"Local Disk (C:) mount failed"); }
                return true;
            }
            if my>=list_top+201&&my<list_top+229{
                push_hist(); set_root(); status(b"AetherFS (A:) opened"); return true;
            }
            if my>=list_top+261&&my<list_top+289{
                status(b"Network: not available"); return true;
            }
            if my>=list_top+293&&my<list_top+321{
                status(b"Devices: open Device Manager from Start"); return true;
            }
            return false;
        }
        if VIEW==VIEW_TEXT{return true;}
    }
    false
}
fn go_back(){
    unsafe{
        if VIEW==VIEW_NTFS{
            if NTFS_DEPTH>0{ go_up(); status(b"Back"); }
            else { VIEW=VIEW_DISK; SEL=-1; status(b"Back to partitions"); }
            return;
        }
        if VIEW==VIEW_FAT{ VIEW=VIEW_DISK; SEL=-1; status(b"Back to partitions"); return; }
        if VIEW==VIEW_DISK{ VIEW=VIEW_COMPUTER; SEL=-1; COMPUTER_SEL=-1; status(b"Computer"); return; }
        if VIEW==VIEW_TEXT{
            let back = PREVIEW_FROM;
            VIEW = if back==VIEW_NTFS || back==VIEW_FAT || back==VIEW_ROOT || back==VIEW_DISK {
                back
            } else {
                VIEW_NTFS // safest: most previews come from disk
            };
            // keep NTFS_CWD_REF / DEPTH / list state intact
            if VIEW==VIEW_NTFS { NTFS_VIEW_DIRTY=true; }
            SEL=-1;
            status(b"Back");
            return;
        }
        if VIEW==VIEW_ROOT{
            if HIST_I>0{
                HIST_I-=1;
                let mut i=0; while i<32{CWD[i]=HIST[HIST_I][i];i+=1;}
                CWD_LEN=HIST_LEN[HIST_I]; SEL=-1; status(b"Back");
            } else { VIEW=VIEW_COMPUTER; SEL=-1; status(b"Computer"); }
            return;
        }
        VIEW=VIEW_COMPUTER; SEL=-1; status(b"Computer");
    }
}
fn go_forward(){
    unsafe{
        if VIEW==VIEW_ROOT && HIST_I+1<HIST_N{
            HIST_I+=1;
            let mut i=0; while i<32{CWD[i]=HIST[HIST_I][i];i+=1;}
            CWD_LEN=HIST_LEN[HIST_I]; SEL=-1; status(b"Forward");
        } else { status(b"No forward history"); }
    }
}
fn go_up(){
    unsafe{
        // CRITICAL: no disk IO here. Post-media AHCI reads caused AH532 reboot.
        // Only change VIEW / path state; list is refreshed on next open/F5.
        let _ = crate::drivers::audio::playback_stop();
        if VIEW==VIEW_NTFS{
            if NTFS_DEPTH>0 && NTFS_DEPTH<=8{
                NTFS_DEPTH-=1;
                let pref = NTFS_PARENT[NTFS_DEPTH];
                if pref != 0 && pref != 0xFFFF_FFFF {
                    NTFS_CWD_REF = pref;
                    // No list_directory here - disk IO after media reboots AH532.
                    // Empty list until F5 / Refresh.
                    NTFS_VIEW_DIRTY = false;
                    NTFS_VIEW_COUNT = 0;
                    NTFS_SCROLL = 0;
                    SEL = -1;
                    status(b"Up - press F5 to list");
                    return;
                }
            }
            VIEW=VIEW_DISK; SEL=-1; NTFS_DEPTH=0; NTFS_CWD_REF=5;
            status(b"Partitions");
            return;
        }
        if VIEW==VIEW_FAT{
            if fs_fat::is_mounted() && !fs_fat::at_root() {
                fs_fat::soft_root();
                SEL=-1;
                status(b"FAT root (press F5)");
            } else {
                VIEW=VIEW_DISK; SEL=-1;
                status(b"Partitions");
            }
            return;
        }
        if VIEW==VIEW_TEXT{
            let back = PREVIEW_FROM;
            VIEW = if back==VIEW_NTFS || back==VIEW_FAT || back==VIEW_ROOT || back==VIEW_DISK {
                back
            } else { VIEW_DISK };
            if VIEW==VIEW_NTFS { NTFS_VIEW_DIRTY=true; }
            SEL=-1; status(b"Back"); return;
        }
        if VIEW==VIEW_DISK{
            VIEW=VIEW_COMPUTER; SEL=-1; COMPUTER_SEL=-1;
            status(b"Computer"); return;
        }
        if VIEW!=VIEW_ROOT{
            VIEW=VIEW_COMPUTER; SEL=-1; CWD[0]=b'/'; CWD_LEN=1;
            status(b"Computer"); return;
        }
        if CWD_LEN<=1{
            VIEW=VIEW_COMPUTER; SEL=-1;
            status(b"Computer"); return;
        }
        let mut i=CWD_LEN;
        while i>1{
            i-=1;
            if CWD[i]==b'/'{
                CWD_LEN=if i==0{1}else{i};
                if CWD_LEN==0{CWD_LEN=1;}
                SEL=-1; VIEW=VIEW_ROOT; status(b"Up"); return;
            }
        }
        CWD[0]=b'/'; CWD_LEN=1; SEL=-1; status(b"Up");
    }
}
