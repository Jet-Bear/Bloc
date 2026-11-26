// Lo que hace falta del control de texto enriquecido de Windows (msftedit.dll, RichEdit 5): windows-sys no lo trae.
#![allow(non_snake_case, clippy::upper_case_acronyms)] // los nombres de Windows tal cual

pub const CLASE: &str = "RICHEDIT50W";

pub const EM_EXGETSEL: u32 = 0x434;
pub const EM_EXLIMITTEXT: u32 = 0x435;
pub const EM_EXSETSEL: u32 = 0x437;
pub const EM_FORMATRANGE: u32 = 0x439;
pub const EM_GETEVENTMASK: u32 = 0x43B;
pub const EM_SETBKGNDCOLOR: u32 = 0x443;
pub const EM_GETCHARFORMAT: u32 = 0x43A;
pub const EM_SETCHARFORMAT: u32 = 0x444;
pub const EM_SETEVENTMASK: u32 = 0x445;
pub const EM_SETTARGETDEVICE: u32 = 0x448;
pub const EM_STREAMIN: u32 = 0x449;
pub const EM_GETTEXTRANGE: u32 = 0x44B;
pub const EM_SETUNDOLIMIT: u32 = 0x452;
pub const EM_REDO: u32 = 0x454;
pub const EM_CANREDO: u32 = 0x455;
pub const EM_STOPGROUPTYPING: u32 = 0x458;
pub const EM_GETTEXTEX: u32 = 0x45E;
pub const EM_GETTEXTLENGTHEX: u32 = 0x45F;
pub const EM_SETTEXTEX: u32 = 0x461;
pub const EM_SETLANGOPTIONS: u32 = 0x478;
pub const EM_GETLANGOPTIONS: u32 = 0x479;
pub const EM_SETEDITSTYLE: u32 = 0x4CC;
pub const EM_GETSCROLLPOS: u32 = 0x4DD;
pub const EM_SETSCROLLPOS: u32 = 0x4DE;
pub const EM_GETZOOM: u32 = 0x4E0;
pub const EM_EXLINEFROMCHAR: u32 = 0x436;
pub const EM_SETZOOM: u32 = 0x4E1;

pub const ENM_CHANGE: u32 = 0x1;
pub const ENM_SELCHANGE: u32 = 0x80000;
pub const EN_SELCHANGE: u32 = 0x702;

pub const SF_TEXT: usize = 0x1;
pub const SF_RTF: usize = 0x2;
pub const SF_UNICODE: usize = 0x10;
pub const SFF_SELECTION: usize = 0x8000;
pub const ST_DEFAULT: u32 = 0;
pub const GTL_PRECISE: u32 = 0x2;
pub const GTL_NUMCHARS: u32 = 0x8;

pub const IMF_AUTOKEYBOARD: isize = 0x1;
pub const IMF_SPELLCHECKING: isize = 0x800;
pub const SES_USECTF: usize = 0x10000;
pub const SES_CTFALLOWPROOFING: usize = 0x800000;

pub const ES_NOOLEDRAGDROP: u32 = 0x8;


pub const CFM_BOLD: u32 = 0x1;
pub const CFM_ITALIC: u32 = 0x2;
pub const CFM_UNDERLINE: u32 = 0x4;
pub const CFM_STRIKEOUT: u32 = 0x8;
pub const CFM_BACKCOLOR: u32 = 0x4000000;
pub const CFM_FACE: u32 = 0x20000000;
pub const CFM_COLOR: u32 = 0x40000000;
pub const CFM_SIZE: u32 = 0x80000000;
pub const CFE_AUTOBACKCOLOR: u32 = 0x4000000;
pub const CFM_UNDERLINETYPE: u32 = 0x800000;
pub const CFM_SPACING: u32 = 0x200000;
pub const CFM_HIDDEN: u32 = 0x100;
pub const CFE_BOLD: u32 = 0x1;
pub const CFE_ITALIC: u32 = 0x2;
pub const CFE_UNDERLINE: u32 = 0x4;
pub const CFE_STRIKEOUT: u32 = 0x8;
pub const CFE_HIDDEN: u32 = 0x100;
pub const CFU_UNDERLINE: u8 = 1;
pub const CFU_UNDERLINETHICK: u8 = 9;
pub const SCF_DEFAULT: usize = 0x0;
pub const SCF_SELECTION: usize = 0x1;
pub const SCF_ALL: usize = 0x4;

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct CHARRANGE {
    pub cpMin: i32,
    pub cpMax: i32,
}

#[repr(C)]
pub struct TEXTRANGEW {
    pub chrg: CHARRANGE,
    pub lpstrText: *mut u16,
}

// richedit.h empaqueta sus estructuras a 4 bytes: en 64 bits, las que llevan un puntero detrás de un número de 4
// bytes no tienen el hueco que pondría Rust.
#[repr(C, packed(4))]
pub struct GETTEXTEX {
    pub cb: u32,
    pub flags: u32,
    pub codepage: u32,
    pub lpDefaultChar: *const u8,
    pub lpUsedDefChar: *mut i32,
}

#[repr(C)]
pub struct GETTEXTLENGTHEX {
    pub flags: u32,
    pub codepage: u32,
}

#[repr(C)]
pub struct SETTEXTEX {
    pub flags: u32,
    pub codepage: u32,
}

pub type LeerCallback = unsafe extern "system" fn(usize, *mut u8, i32, *mut i32) -> u32;

#[repr(C, packed(4))]
pub struct EDITSTREAM {
    pub dwCookie: usize,
    pub dwError: u32,
    pub pfnCallback: Option<LeerCallback>,
}

#[repr(C)]
pub struct CHARFORMAT2W {
    pub cbSize: u32,
    pub dwMask: u32,
    pub dwEffects: u32,
    pub yHeight: i32,
    pub yOffset: i32,
    pub crTextColor: u32,
    pub bCharSet: u8,
    pub bPitchAndFamily: u8,
    pub szFaceName: [u16; 32],
    pub wWeight: u16,
    pub sSpacing: i16,
    pub crBackColor: u32,
    pub lcid: u32,
    pub dwCookie: u32,
    pub sStyle: i16,
    pub wKerning: u16,
    pub bUnderlineType: u8,
    pub bAnimation: u8,
    pub bRevAuthor: u8,
    pub bUnderlineColor: u8,
}

pub const EM_GETPARAFORMAT: u32 = 0x43D;
pub const EM_SETPARAFORMAT: u32 = 0x447;
pub const PFM_STARTINDENT: u32 = 0x1;
pub const PFM_RIGHTINDENT: u32 = 0x2;
pub const PFM_OFFSET: u32 = 0x4;
pub const PFM_ALIGNMENT: u32 = 0x8;
pub const PFM_NUMBERING: u32 = 0x20;
pub const PFM_SPACEBEFORE: u32 = 0x40;
pub const PFM_SPACEAFTER: u32 = 0x80;
pub const PFM_LINESPACING: u32 = 0x100;
pub const PFM_NUMBERINGSTYLE: u32 = 0x2000;
pub const PFM_NUMBERINGSTART: u32 = 0x8000;
pub const PFA_LEFT: u16 = 1;
pub const PFA_CENTER: u16 = 3;

#[repr(C)]
pub struct PARAFORMAT2 {
    pub cbSize: u32,
    pub dwMask: u32,
    pub wNumbering: u16,
    pub wEffects: u16,
    pub dxStartIndent: i32,
    pub dxRightIndent: i32,
    pub dxOffset: i32,
    pub wAlignment: u16,
    pub cTabCount: i16,
    pub rgxTabs: [i32; 32],
    pub dySpaceBefore: i32,
    pub dySpaceAfter: i32,
    pub dyLineSpacing: i32,
    pub sStyle: i16,
    pub bLineSpacingRule: u8,
    pub bOutlineLevel: u8,
    pub wShadingWeight: u16,
    pub wShadingStyle: u16,
    pub wNumberingStart: u16,
    pub wNumberingStyle: u16,
    pub wNumberingTab: u16,
    pub wBorderSpace: u16,
    pub wBorderWidth: u16,
    pub wBorders: u16,
}

#[repr(C)]
pub struct FORMATRANGE {
    pub hdc: windows_sys::Win32::Graphics::Gdi::HDC,
    pub hdcTarget: windows_sys::Win32::Graphics::Gdi::HDC,
    pub rc: windows_sys::Win32::Foundation::RECT,
    pub rcPage: windows_sys::Win32::Foundation::RECT,
    pub chrg: CHARRANGE,
}
