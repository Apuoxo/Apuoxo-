//! Native Aether media player core.
//! Streaming VFS reader + bounded PCM buffer. The decoder boundary is format
//! aware; WAV/PCM is the first native decoder and feeds the future HDA backend.

use crate::fs;
use crate::serial;
use crate::media_builtin;

const MAX_PATH: usize = 96;
const HEADER_BUF: usize = 4096;
const PCM_BUF: usize = 32768;
const MP3_INPUT_BUF: usize = 16384;
const MP3_FRAME_SAMPLES: usize = 1152 * 2;
const MAX_PLAYLIST: usize = 16;

#[derive(Clone, Copy, PartialEq)]
pub enum State { Empty, Stopped, Playing, Paused, Error }

#[derive(Clone, Copy, PartialEq)]
pub enum Format { Unknown, WavPcm, Mp3 }

static mut STATE: State = State::Empty;
static mut FORMAT: Format = Format::Unknown;
static mut PATH: [u8; MAX_PATH] = [0; MAX_PATH];
static mut PATH_LEN: usize = 0;
static mut TITLE: [u8; MAX_PATH] = [0; MAX_PATH];
static mut TITLE_LEN: usize = 0;
static mut FILE_SIZE: usize = 0;
static mut SAMPLE_RATE: u32 = 0;
static mut CHANNELS: u16 = 0;
static mut BITS: u16 = 0;
static mut DATA_OFF: usize = 0;
static mut DATA_LEN: usize = 0;
static mut PCM_FILE_POS: usize = 0;
static mut PCM_READY: usize = 0;
static mut VOLUME: u8 = 100;
static mut MUTED: bool = false;
static mut REPEAT: bool = false;
static mut SHUFFLE: bool = false;
static mut LAST_ERROR: u8 = 0;
static mut PCM: [u8; PCM_BUF] = [0; PCM_BUF];
static mut PLAYLIST: [[u8; MAX_PATH]; MAX_PLAYLIST] = [[0; MAX_PATH]; MAX_PLAYLIST];
static mut PL_LEN: [usize; MAX_PLAYLIST] = [0; MAX_PLAYLIST];
static mut PL_COUNT: usize = 0;
static mut PL_INDEX: usize = 0;
static mut BUILTIN_ACTIVE: u8 = 0;
static mut SOURCE_NTFS: bool = false;
static mut NTFS_MFT_REF: u32 = 0;
static mut ALARM_SYNTH: bool = false;
static mut SELECTED_BUILTIN: usize = 0;
static mut MP3_DECODER: crate::mp3_decoder::Decoder = crate::mp3_decoder::Decoder::new();
static mut MP3_INPUT: [u8; MP3_INPUT_BUF] = [0; MP3_INPUT_BUF];
static mut MP3_INPUT_LEN: usize = 0;
static mut MP3_FILE_POS: usize = 0;
static mut MP3_FRAME: [f32; MP3_FRAME_SAMPLES] = [0.0; MP3_FRAME_SAMPLES];

fn le16(b: &[u8], p: usize) -> u16 { (b[p] as u16) | ((b[p+1] as u16) << 8) }
fn le32(b: &[u8], p: usize) -> u32 {
    (b[p] as u32) | ((b[p+1] as u32) << 8) | ((b[p+2] as u32) << 16) | ((b[p+3] as u32) << 24)
}
fn copy_bytes(dst: &mut [u8], src: &[u8]) -> usize {
    let n=dst.len().min(src.len()); let mut i=0; while i<n { dst[i]=src[i]; i+=1; } n
}
fn four(b:&[u8],p:usize,s:&[u8;4])->bool { p+4<=b.len() && b[p..p+4]==s[..] }

fn parse_wav_bytes(b:&[u8])->Option<(usize,u32,u16,u16,usize,usize)> {
    if b.len()<12 || !four(b,0,b"RIFF") || !four(b,8,b"WAVE") { return None; }
    let mut p=12usize; let mut rate=0u32; let mut ch=0u16; let mut bits=0u16;
    let mut data_off=0usize; let mut data_len=0usize; let mut fmt=false;
    while p+8<=b.len() {
        let sz=le32(b,p+4) as usize; let body=p+8;
        let end=body.checked_add(sz)?;
        if end>b.len() { return None; }
        if four(b,p,b"fmt ") && sz>=16 {
            if le16(b,body)!=1 { return None; }
            ch=le16(b,body+2); rate=le32(b,body+4); bits=le16(b,body+14);
            if rate==0 || (ch!=1 && ch!=2) || (bits!=8 && bits!=16) { return None; }
            fmt=true;
        } else if four(b,p,b"data") {
            data_off=body; data_len=sz.min(b.len().saturating_sub(body));
            let frame=((ch as usize)*(bits as usize))/8;
            if frame==0 { return None; }
            data_len-=data_len%frame;
            return if fmt { Some((b.len(),rate,ch,bits,data_off,data_len)) } else { None };
        }
        p=end+(sz&1);
    }
    None
}

pub fn builtin_count()->usize { 2 }
pub fn builtin_name(i:usize)->&'static str {
    match i { 0=>"TEST.WAV", 1=>"BEETHOVEN.MP3", _=>"" }
}
pub fn builtin_kind(i:usize)->&'static str {
    match i { 0=>"WAV PCM", 1=>"MP3", _=>"" }
}
pub fn builtin_bytes(i:usize)->&'static [u8] {
    match i { 0=>media_builtin::TEST_WAV, 1=>media_builtin::TEST_MP3, _=>&[] }
}
pub fn selected_builtin()->usize { unsafe { SELECTED_BUILTIN } }
pub fn select_builtin(i:usize)->bool {
    if i>=builtin_count() { return false; }
    unsafe { SELECTED_BUILTIN=i; }
    true
}

pub fn builtin_path(i:usize)->&'static str {
    match i { 0=>"/MEDIA/TEST.WAV", 1=>"/MEDIA/BEETHOVEN.MP3", _=>"" }
}

pub fn open_embedded_wav()->bool {
    let p="/MEDIA/TEST.WAV";
    let bytes=media_builtin::TEST_WAV;
    if let Some((size,rate,ch,bits,data,len))=parse_wav_bytes(bytes) {
        unsafe {
            PATH_LEN=copy_bytes(&mut PATH,p.as_bytes());
            TITLE_LEN=PATH_LEN;
            let mut j=0; while j<TITLE_LEN { TITLE[j]=PATH[j]; j+=1; }
            FILE_SIZE=size; SAMPLE_RATE=rate; CHANNELS=ch; BITS=bits;
            DATA_OFF=data; DATA_LEN=len; PCM_FILE_POS=0; PCM_READY=0;
            FORMAT=Format::WavPcm; LAST_ERROR=0; STATE=State::Stopped;
            BUILTIN_ACTIVE=1;
        }
        serial::write_str("[MEDIA] embedded TEST.WAV opened directly\n");
        true
    } else {
        serial::write_str("[MEDIA] embedded TEST.WAV parse failed\\n");
        false
    }
}

pub fn open_builtin(i:usize)->bool {
    unsafe { SOURCE_NTFS = false; NTFS_MFT_REF = 0; }
    let p=builtin_path(i);
    if p.is_empty(){return false;}
    add_to_playlist(p);
    if open(p){
        unsafe { BUILTIN_ACTIVE = i as u8 + 1; }
        serial::write_str("[MEDIA] opened AetherFS media file\n");
        return true;
    }

    // Built-in smoke media is embedded in the kernel image.  Keep AUD2
    // independent of whether /MEDIA is mounted/populated in AetherFS.
    let bytes=builtin_bytes(i);
    if let Some((size,rate,ch,bits,data,len))=parse_wav_bytes(bytes) {
        unsafe {
            PATH_LEN=copy_bytes(&mut PATH,p.as_bytes());
            TITLE_LEN=PATH_LEN;
            let mut j=0; while j<TITLE_LEN { TITLE[j]=PATH[j]; j+=1; }
            FILE_SIZE=size; SAMPLE_RATE=rate; CHANNELS=ch; BITS=bits;
            DATA_OFF=data; DATA_LEN=len; PCM_FILE_POS=0; PCM_READY=0;
            FORMAT=Format::WavPcm; LAST_ERROR=0; STATE=State::Stopped;
            BUILTIN_ACTIVE=i as u8 + 1;
        }
        serial::write_str("[MEDIA] opened embedded WAV smoke source\n");
        return true;
    }
    if i==1 {
        if open_mp3_common(p, bytes.len()) {
            unsafe { BUILTIN_ACTIVE = 2; }
            return true;
        }
    }
    false
}

fn parse_wav(path:&str)->Option<(usize,u32,u16,u16,usize,usize)> {
    let size=fs::file_size(path)?;
    if size<12 { return None; }
    let mut h=[0u8;HEADER_BUF];
    let n=fs::read_range(path,0,&mut h)?;
    if n<12 || !four(&h,0,b"RIFF") || !four(&h,8,b"WAVE") { return None; }
    let mut p=12usize; let mut rate=0u32; let mut ch=0u16; let mut bits=0u16;
    let mut data_off=0usize; let mut data_len=0usize; let mut fmt=false;
    while p+8<=n {
        let sz=le32(&h,p+4) as usize; let body=p+8;
        let end=body.checked_add(sz)?;
        if end>size || end>n { 
            // Header chunks must be contained in the bounded header buffer.
            // If the data chunk starts beyond the header window, reject it
            // instead of reading unbounded metadata.
            return None;
        }
        if four(&h,p,b"fmt ") && sz>=16 {
            if le16(&h,body)!=1 { return None; }
            ch=le16(&h,body+2); rate=le32(&h,body+4); bits=le16(&h,body+14);
            if rate==0 || (ch!=1 && ch!=2) || (bits!=8 && bits!=16) { return None; }
            fmt=true;
        } else if four(&h,p,b"data") {
            data_off=body; data_len=sz; 
            if data_off.checked_add(data_len)? > size { return None; }
            data_len=(size-data_off).min(data_len);
            data_len-=data_len%(((ch as usize)*(bits as usize))/8).max(1);
            return if fmt { Some((size,rate,ch,bits,data_off,data_len)) } else { None };
        }
        p=end+(sz&1);
    }
    None
}

pub fn init() {
    unsafe {
        STATE=State::Empty; FORMAT=Format::Unknown; SELECTED_BUILTIN=0; PATH_LEN=0; TITLE_LEN=0;
        FILE_SIZE=0; SAMPLE_RATE=0; CHANNELS=0; BITS=0; DATA_OFF=0; DATA_LEN=0;
        PCM_FILE_POS=0; PCM_READY=0; MP3_INPUT_LEN=0; MP3_FILE_POS=0; MP3_DECODER=crate::mp3_decoder::Decoder::new(); VOLUME=100; MUTED=false;
        REPEAT=false; SHUFFLE=false; LAST_ERROR=0; PL_COUNT=0; PL_INDEX=0; BUILTIN_ACTIVE=0;
    }
    serial::write_str("[MEDIA] native streaming player core ready\n");
}

pub fn state()->State { unsafe{STATE} }
pub fn format()->Format { unsafe{FORMAT} }
pub fn volume()->u8 { unsafe{VOLUME} }
pub fn muted()->bool { unsafe{MUTED} }
pub fn repeat()->bool { unsafe{REPEAT} }
pub fn shuffle()->bool { unsafe{SHUFFLE} }
pub fn sample_rate()->u32 { unsafe{SAMPLE_RATE} }
pub fn channels()->u16 { unsafe{CHANNELS} }
pub fn bits()->u16 { unsafe{BITS} }
pub fn position_bytes()->usize { unsafe{if FORMAT==Format::Mp3 { MP3_FILE_POS } else { PCM_FILE_POS }} }
pub fn data_bytes()->usize { unsafe{DATA_LEN} }
pub fn file_size()->usize { unsafe{FILE_SIZE} }
pub fn error()->u8 { unsafe{LAST_ERROR} }
pub fn title(out:&mut [u8])->usize { unsafe{copy_bytes(out,&TITLE[..TITLE_LEN])} }
pub fn path(out:&mut [u8])->usize { unsafe{copy_bytes(out,&PATH[..PATH_LEN])} }

fn is_mp3_path(path:&str)->bool {
    let b=path.as_bytes();
    if b.len()<4 { return false; }
    let a=b[b.len()-4];
    let c=b[b.len()-3];
    let d=b[b.len()-2];
    let e=b[b.len()-1];
    (a==b'.') && ((c|0x20)==b'm') && ((d|0x20)==b'p') && ((e|0x20)==b'3')
}

fn open_mp3_common(path:&str, bytes_len:usize)->bool {
    if bytes_len==0 { unsafe{STATE=State::Error; LAST_ERROR=2;} return false; }
    unsafe {
        PATH_LEN=copy_bytes(&mut PATH,path.as_bytes());
        TITLE_LEN=PATH_LEN.min(TITLE.len());
        let mut i=0; while i<TITLE_LEN { TITLE[i]=PATH[i]; i+=1; }
        FILE_SIZE=bytes_len; SAMPLE_RATE=0; CHANNELS=0; BITS=16;
        DATA_OFF=0; DATA_LEN=bytes_len; PCM_FILE_POS=0; PCM_READY=0;
        MP3_INPUT_LEN=0; MP3_FILE_POS=0;
        MP3_DECODER=crate::mp3_decoder::Decoder::new();
        FORMAT=Format::Mp3; LAST_ERROR=0; STATE=State::Stopped;
    }
    serial::write_str("[MEDIA] MP3 decoder opened\n");
    true
}

fn refill_mp3()->usize {
    unsafe {
        if FORMAT!=Format::Mp3 { PCM_READY=0; return 0; }

        // Keep PCM as a continuous byte FIFO. HDA consumes fixed 4096-byte
        // periods, while an MP3 frame normally decodes to 4608 bytes
        // (1152 stereo samples). Never expose a partial frame to HDA.
        while PCM_READY < 16384 && MP3_FILE_POS < FILE_SIZE {
            let remain=FILE_SIZE-MP3_FILE_POS;
            let n=remain.min(MP3_INPUT_BUF);
            if n==0 { break; }

            let got=if BUILTIN_ACTIVE!=0 {
                let bi=(BUILTIN_ACTIVE-1) as usize;
                let src=builtin_bytes(bi);
                if MP3_FILE_POS>=src.len() { None } else {
                    let avail=(src.len()-MP3_FILE_POS).min(n);
                    let mut i=0;
                    while i<avail { MP3_INPUT[i]=src[MP3_FILE_POS+i]; i+=1; }
                    Some(avail)
                }
            } else if SOURCE_NTFS {
                crate::fs_ntfs::read_file_range(NTFS_MFT_REF, MP3_FILE_POS, &mut MP3_INPUT[..n])
            } else {
                fs::read_range(
                    core::str::from_utf8_unchecked(&PATH[..PATH_LEN]),
                    MP3_FILE_POS,
                    &mut MP3_INPUT[..n]
                )
            };

            let got=match got {
                Some(v) if v>0=>v,
                _=>{LAST_ERROR=3;STATE=State::Error;break;}
            };
            MP3_INPUT_LEN=got;

            // Decode from the current input window. minimp3 may consume less
            // than the bytes supplied, so preserve the unconsumed tail for
            // the next decode instead of discarding it. Losing a partial MP3
            // frame here causes periodic decoder resync noise.
            let (consumed, info)=MP3_DECODER.decode(
                &MP3_INPUT[..MP3_INPUT_LEN],
                &mut MP3_FRAME
            );

            if consumed>0 {
                MP3_FILE_POS=MP3_FILE_POS.saturating_add(consumed);
            }

            let info=match info {
                Some(v)=>v,
                None=>{
                    if consumed==0 && MP3_FILE_POS < FILE_SIZE {
                        LAST_ERROR=4;
                        STATE=State::Error;
                    }
                    break;
                }
            };

            SAMPLE_RATE=info.sample_rate;
            CHANNELS=info.channels.num() as u16;
            BITS=16;

            // minimp3 reports samples per channel. PCM is interleaved,
            // so stereo needs samples * channels values.
            let channels=info.channels.num() as usize;
            let sample_count=info.samples_produced.min(MP3_FRAME_SAMPLES / channels.max(1));
            let sample_values=sample_count.saturating_mul(channels);
            let bytes_needed=sample_values.saturating_mul(2);
            let space=PCM_BUF.saturating_sub(PCM_READY);
            let bytes=bytes_needed.min(space);
            let mut src=0usize;
            let mut dst=PCM_READY;

            while src < bytes {
                let mut v=MP3_FRAME[src/2];
                if v>1.0 { v=1.0; } else if v < -1.0 { v=-1.0; }
                let s=(v*32767.0) as i16;
                let u=s as u16;
                PCM[dst]=(u&0xFF) as u8;
                PCM[dst+1]=(u>>8) as u8;
                src+=2;
                dst+=2;
            }
            PCM_READY=dst;

            if bytes==0 {
                LAST_ERROR=5;
                STATE=State::Error;
                break;
            }
        }

        PCM_READY
    }
}
pub fn open(path:&str)->bool {
    if is_mp3_path(path) {
        let size=match fs::file_size(path) {
            Some(v)=>v,
            None=>{unsafe{STATE=State::Error;FORMAT=Format::Unknown;LAST_ERROR=1;} return false;}
        };
        unsafe { BUILTIN_ACTIVE=0; }
        return open_mp3_common(path,size);
    }

    let (size,rate,ch,bits,data,len)=match parse_wav(path) {
        Some(v)=>v,
        None=>{unsafe{STATE=State::Error;FORMAT=Format::Unknown;LAST_ERROR=2;} serial::write_str("[MEDIA] unsupported or malformed WAV
"); return false;}
    };
    unsafe {
        BUILTIN_ACTIVE=0;
        PATH_LEN=copy_bytes(&mut PATH,path.as_bytes());
        TITLE_LEN=PATH_LEN.min(TITLE.len());
        let mut i=0; while i<TITLE_LEN { TITLE[i]=PATH[i]; i+=1; }
        FILE_SIZE=size; SAMPLE_RATE=rate; CHANNELS=ch; BITS=bits;
        DATA_OFF=data; DATA_LEN=len; PCM_FILE_POS=0; PCM_READY=0;
        FORMAT=Format::WavPcm; LAST_ERROR=0; STATE=State::Stopped;
    }
    serial::write_str("[MEDIA] WAV PCM opened
");
    true
}

pub fn refill_pcm()->usize {
    if unsafe { FORMAT==Format::Mp3 } { return refill_mp3(); }
    unsafe {
        if FORMAT!=Format::WavPcm || DATA_LEN==0 || PCM_FILE_POS>=DATA_LEN { PCM_READY=0; return 0; }
        let frame=((CHANNELS as usize)*(BITS as usize))/8;
        if frame==0 { return 0; }
        let remain=DATA_LEN-PCM_FILE_POS;
        let n=PCM_BUF.min(remain);
        let n=n-(n%frame);
        if n==0 { PCM_READY=0; return 0; }
        let got = if BUILTIN_ACTIVE != 0 {
            let bi=(BUILTIN_ACTIVE-1) as usize;
            let src=builtin_bytes(bi);
            let off=DATA_OFF + PCM_FILE_POS;
            if off < src.len() {
                let avail=(src.len()-off).min(n);
                let mut i=0; while i<avail { PCM[i]=src[off+i]; i+=1; }
                Some(avail)
            } else { None }
        } else if SOURCE_NTFS {
            crate::fs_ntfs::read_file_range(
                NTFS_MFT_REF,
                DATA_OFF + PCM_FILE_POS,
                &mut PCM[..n]
            )
        } else {
            fs::read_range(
                core::str::from_utf8_unchecked(&PATH[..PATH_LEN]),
                DATA_OFF + PCM_FILE_POS,
                &mut PCM[..n]
            )
        };
        match got {
            Some(got) if got>0 => { PCM_READY=got; got },
            _ => { PCM_READY=0; LAST_ERROR=3; STATE=State::Error; 0 }
        }
    }
}

pub fn pcm_buffer(out:&mut [u8])->usize {
    unsafe {
        let n=out.len().min(PCM_READY);
        let mut i=0;
        // Apply the player volume/mute state in the PCM sink path.
        // The HDA codec remains at the known-good hardware gain; UI volume
        // therefore changes only the samples that are sent to DMA.
        if MUTED || VOLUME == 0 {
            while i < n { out[i] = 0; i += 1; }
        } else if VOLUME >= 100 {
            while i < n { out[i] = PCM[i]; i += 1; }
        } else {
            // Current native HDA playback is 16-bit PCM. Scale complete
            // little-endian samples without changing the stream format.
            while i + 1 < n {
                let raw = (PCM[i] as u16) | ((PCM[i + 1] as u16) << 8);
                let sample = raw as i16 as i32;
                let scaled = sample * (VOLUME as i32) / 100;
                let s = scaled as i16;
                let u = s as u16;
                out[i] = (u & 0xFF) as u8;
                out[i + 1] = (u >> 8) as u8;
                i += 2;
            }
            while i < n { out[i] = PCM[i]; i += 1; }
        }
        n
    }
}

/// Called by the PCM sink after it has consumed n bytes.
pub fn consume_pcm(n:usize) {
    unsafe {
        let n=n.min(PCM_READY);
        if n>0 && n<PCM_READY {
            // HDA consumed the first part of the FIFO. Move the remaining
            // decoded PCM to the front before appending another MP3 frame.
            let remain=PCM_READY-n;
            let mut i=0;
            while i<remain {
                PCM[i]=PCM[n+i];
                i+=1;
            }
            PCM_READY=remain;
        } else {
            PCM_READY=0;
        }

        if FORMAT!=Format::Mp3 {
            PCM_FILE_POS+=n;
            if PCM_READY==0 && PCM_FILE_POS<DATA_LEN {
                let _=refill_pcm();
            }
        } else {
            // MP3_FILE_POS is advanced by the decoder, not by PCM playback.
            // Refill whenever the FIFO drops below one HDA period.
            if PCM_READY<16384 && MP3_FILE_POS<DATA_LEN {
                let _=refill_pcm();
            }
        }

        if FORMAT==Format::Mp3 {
            if MP3_FILE_POS>=DATA_LEN && PCM_READY==0 {
                if REPEAT {
                    MP3_FILE_POS=0;
                    MP3_INPUT_LEN=0;
                    MP3_DECODER=crate::mp3_decoder::Decoder::new();
                    let _=refill_pcm();
                } else {
                    STATE=State::Stopped;
                }
            }
        } else if PCM_FILE_POS>=DATA_LEN {
            if REPEAT {
                PCM_FILE_POS=0;
                PCM_READY=0;
                let _=refill_pcm();
            } else {
                STATE=State::Stopped;
                PCM_READY=0;
            }
        }
    }
}

pub fn play(){unsafe{if DATA_LEN>0 && (STATE==State::Stopped||STATE==State::Paused){STATE=State::Playing;let _=refill_pcm();if !crate::drivers::audio::playback_start(SAMPLE_RATE,CHANNELS,BITS){STATE=State::Error;}}}}
pub fn pause(){unsafe{if STATE==State::Playing{crate::drivers::audio::playback_stop();STATE=State::Paused;}}}
pub fn stop(){crate::drivers::audio::playback_stop();unsafe{PCM_FILE_POS=0;PCM_READY=0;STATE=State::Stopped;}}
pub fn toggle_play(){unsafe{if STATE==State::Playing{crate::drivers::audio::playback_stop();STATE=State::Paused}else if DATA_LEN>0{STATE=State::Playing;let _=refill_pcm();if !crate::drivers::audio::playback_start(SAMPLE_RATE,CHANNELS,BITS){STATE=State::Error;}}}}

pub fn seek_permille(v:u16){
    unsafe{
        if DATA_LEN==0{return;}
        let x=v.min(1000) as usize;
        if FORMAT==Format::Mp3 {
            MP3_FILE_POS=(DATA_LEN*x)/1000;
            PCM_READY=0;
            MP3_INPUT_LEN=0;
            MP3_DECODER=crate::mp3_decoder::Decoder::new();
            if STATE==State::Playing { let _=refill_pcm(); }
            return;
        }
        let frame=((CHANNELS as usize)*(BITS as usize))/8;
        if frame==0{return;}
        PCM_FILE_POS=((DATA_LEN*x)/1000/frame)*frame;
        PCM_READY=0;
        if STATE==State::Playing { let _=refill_pcm(); }
    }
}
pub fn set_volume(v:u8){unsafe{VOLUME=v.min(100);}}
pub fn volume_up(){unsafe{VOLUME=VOLUME.saturating_add(5).min(100);}}
pub fn volume_down(){unsafe{VOLUME=VOLUME.saturating_sub(5);}}
pub fn toggle_mute(){unsafe{MUTED=!MUTED;}}
pub fn toggle_repeat(){unsafe{REPEAT=!REPEAT;}}
pub fn toggle_shuffle(){unsafe{SHUFFLE=!SHUFFLE;}}

pub fn add_to_playlist(path:&str)->bool{
    unsafe{
        let n=path.as_bytes().len().min(MAX_PATH);
        let mut i=0usize;
        while i<PL_COUNT{
            if PL_LEN[i]==n {
                let mut same=true;
                let mut j=0usize;
                while j<n { if PLAYLIST[i][j]!=path.as_bytes()[j] {same=false;break;} j+=1; }
                if same { PL_INDEX=i; return true; }
            }
            i+=1;
        }
        if PL_COUNT>=MAX_PLAYLIST{return false;}
        PL_LEN[PL_COUNT]=copy_bytes(&mut PLAYLIST[PL_COUNT],path.as_bytes());
        PL_INDEX=PL_COUNT;
        PL_COUNT+=1;
        true
    }
}
pub fn playlist_count()->usize{unsafe{PL_COUNT}}
fn open_playlist_index(i:usize)->bool{
    unsafe{
        if i>=PL_COUNT{return false;}
        let n=PL_LEN[i]; let mut p=[0u8;MAX_PATH]; let mut j=0usize;
        while j<n{p[j]=PLAYLIST[i][j];j+=1;}
        if let Ok(s)=core::str::from_utf8(&p[..n]) { open(s) } else { false }
    }
}
pub fn previous()->bool{
    unsafe{
        if PL_COUNT==0{return false;}
        if PL_INDEX>0{PL_INDEX-=1}else if REPEAT{PL_INDEX=PL_COUNT-1}else{return false;}
        open_playlist_index(PL_INDEX)
    }
}
pub fn next()->bool{
    unsafe{
        if PL_COUNT==0{return false;}
        if SHUFFLE{PL_INDEX=(PL_INDEX.wrapping_mul(7).wrapping_add(3))%PL_COUNT}
        else if PL_INDEX+1<PL_COUNT{PL_INDEX+=1}
        else if REPEAT{PL_INDEX=0}
        else{return false}
        open_playlist_index(PL_INDEX)
    }
}


// ── Explorer / Alarm extensions (native rate; no software time-stretch) ──

pub fn is_audio_path(path: &str) -> bool {
    let b = path.as_bytes();
    if b.len() < 4 { return false; }
    let a = b[b.len()-4];
    let c = b[b.len()-3] | 0x20;
    let d = b[b.len()-2] | 0x20;
    let e = b[b.len()-1] | 0x20;
    if a != b'.' { return false; }
    (c == b'm' && d == b'p' && e == b'3')
        || (c == b'w' && d == b'a' && e == b'v')
}

pub fn builtin_index_for_path(path: &str) -> Option<usize> {
    let b = path.as_bytes();
    let mut start = 0usize;
    let mut i = 0usize;
    while i < b.len() {
        if b[i] == b'/' { start = i + 1; }
        i += 1;
    }
    let leaf = &b[start..];
    let eq = |a: &[u8], name: &[u8]| -> bool {
        if a.len() != name.len() { return false; }
        let mut k = 0usize;
        while k < a.len() {
            let x = if a[k] >= b'a' && a[k] <= b'z' { a[k] - 32 } else { a[k] };
            let y = if name[k] >= b'a' && name[k] <= b'z' { name[k] - 32 } else { name[k] };
            if x != y { return false; }
            k += 1;
        }
        true
    };
    if eq(leaf, b"MOZART.MP3") || eq(leaf, b"MOZART.WAV") {
        Some(0)
    } else {
        None
    }
}

pub fn open_from_explorer(path: &str) -> bool {
    crate::drivers::audio::playback_stop();
    if let Some(i) = builtin_index_for_path(path) {
        unsafe { SOURCE_NTFS = false; NTFS_MFT_REF = 0; }
        return open_builtin(i);
    }
    if open(path) {
        unsafe { SOURCE_NTFS = false; NTFS_MFT_REF = 0; }
        return true;
    }
    false
}

pub fn open_from_ntfs(mft_ref: u32, name: &[u8]) -> bool {
    if mft_ref == 0 { return false; }
    crate::drivers::audio::playback_stop();
    let nlen = name.len().min(90);
    let mut path_buf = [0u8; 96];
    let mut i = 0usize;
    while i < nlen {
        path_buf[i] = name[i];
        i += 1;
    }
    let path = unsafe { core::str::from_utf8_unchecked(&path_buf[..nlen]) };
    if !is_audio_path(path) { return false; }
    let size = match crate::fs_ntfs::file_data_size(mft_ref) {
        Some(s) if s > 0 => s,
        _ => return false,
    };
    unsafe {
        SOURCE_NTFS = true;
        NTFS_MFT_REF = mft_ref;
        BUILTIN_ACTIVE = 0;
    }
    if is_mp3_path(path) {
        return open_mp3_common(path, size);
    }
    let mut hdr = [0u8; 256];
    let n = match crate::fs_ntfs::read_file_range(mft_ref, 0, &mut hdr) {
        Some(v) => v,
        None => return false,
    };
    if let Some((_sz, rate, ch, bits, data, len)) = parse_wav_bytes(&hdr[..n]) {
        let data_len = if data + len <= size { len } else { size.saturating_sub(data) };
        unsafe {
            PATH_LEN = copy_bytes(&mut PATH, path.as_bytes());
            TITLE_LEN = PATH_LEN.min(TITLE.len());
            let mut j = 0;
            while j < TITLE_LEN {
                TITLE[j] = PATH[j];
                j += 1;
            }
            FILE_SIZE = size;
            SAMPLE_RATE = rate;
            CHANNELS = ch;
            BITS = bits;
            DATA_OFF = data;
            DATA_LEN = data_len;
            PCM_FILE_POS = 0;
            PCM_READY = 0;
            FORMAT = Format::WavPcm;
            LAST_ERROR = 0;
            STATE = State::Stopped;
        }
        return true;
    }
    false
}

pub fn start_alarm_sound() -> bool {
    crate::drivers::audio::playback_stop();
    if open_builtin(0) {
        play();
        return true;
    }
    false
}

pub fn stop_alarm_sound() {
    crate::drivers::audio::playback_stop();
    unsafe {
        if STATE == State::Playing || STATE == State::Paused {
            STATE = State::Stopped;
        }
        ALARM_SYNTH = false;
    }
}
