// Lee y escribe respetando la codificación y los saltos de línea del original. Dentro de Bloc todo salto es un
// solo \r, como en el control de texto de Windows; al guardar se ponen los del archivo.
use std::fs::{self, OpenOptions};
use std::hash::{BuildHasher, Hasher};
use std::io::{self, Write};
use std::os::windows::ffi::OsStrExt;
use std::os::windows::fs::{FileTimesExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

use windows_sys::Win32::Globalization::{GetACP, MultiByteToWideChar, WideCharToMultiByte, MB_ERR_INVALID_CHARS};
use windows_sys::Win32::Storage::FileSystem::{GetFileAttributesW, ReplaceFileW, SetFileAttributesW, INVALID_FILE_ATTRIBUTES};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Cod {
    Utf8,
    Utf8Bom,
    Utf16 { be: bool, bom: bool },
    Ansi(u32),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Saltos {
    Crlf,
    Lf,
    Cr,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Aviso {
    Ninguno,
    BytesNoValidos,
    Mezclados,
    Nulos,
}

pub struct Leido {
    pub texto: Vec<u16>,
    pub cod: Cod,
    pub saltos: Saltos,
    pub aviso: Aviso,
    /// Los caracteres nulos se ven y se guardan como espacios: el control de texto no los admite.
    pub nulos: bool,
}

#[derive(Debug)]
pub enum ErrorGuardar {
    SoloLectura,
    Io(io::Error),
    /// Ni el nuevo ni el original están en su sitio: quedan con estos nombres.
    Apartado { original: PathBuf, nuevo: PathBuf },
}

/// La codificación ANSI de este Windows (la de Europa occidental, la rusa, la japonesa...).
pub fn ansi() -> u32 {
    match unsafe { GetACP() } {
        65001 => 1252, // Windows con «UTF-8 en todo el mundo»: ANSI ya no dice nada, se toma la occidental
        cp => cp,
    }
}

pub fn leer(ruta: &Path) -> io::Result<Leido> {
    // Abrir cuesta unas siete veces el tamaño en memoria: más de 256 MB, se dice en vez de quedarse sin ella.
    if fs::metadata(ruta)?.len() > 256 << 20 {
        return Err(io::Error::from_raw_os_error(223)); // ERROR_FILE_TOO_LARGE, con el texto de Windows en su idioma
    }
    Ok(decodificar(&fs::read(ruta)?))
}

pub fn decodificar(b: &[u8]) -> Leido {
    let mut malos = false;
    let (crudo, cod) = if let Some(r) = b.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        let s = String::from_utf8_lossy(r);
        malos = matches!(s, std::borrow::Cow::Owned(_));
        (s.encode_utf16().collect(), Cod::Utf8Bom)
    } else if let Some(r) = b.strip_prefix(&[0xFF, 0xFE]) {
        (desde_utf16(r, false, &mut malos), Cod::Utf16 { be: false, bom: true })
    } else if let Some(r) = b.strip_prefix(&[0xFE, 0xFF]) {
        (desde_utf16(r, true, &mut malos), Cod::Utf16 { be: true, bom: true })
    } else if let Some(be) = parece_utf16(b) {
        (desde_utf16(b, be, &mut malos), Cod::Utf16 { be, bom: false })
    } else if let Ok(s) = std::str::from_utf8(b) {
        (s.encode_utf16().collect(), Cod::Utf8)
    } else if casi_utf8(b) {
        // Un UTF-8 con algún byte roto (un .log cortado a media letra): leído como ANSI, cada ñ saldría «Ã±» sin aviso,
        // y lo que se escribiera luego quedaría en otra codificación que lo de antes.
        malos = true;
        (String::from_utf8_lossy(b).encode_utf16().collect(), Cod::Utf8)
    } else {
        let cp = ansi();
        (desde_ansi(b, cp, &mut malos), Cod::Ansi(cp))
    };
    let (mut crlf, mut lf, mut cr) = (0usize, 0usize, 0usize);
    let mut nulos = false;
    let mut texto = Vec::with_capacity(crudo.len());
    let mut i = 0;
    while i < crudo.len() {
        match crudo[i] {
            13 => {
                if crudo.get(i + 1) == Some(&10) {
                    crlf += 1;
                    i += 1;
                } else {
                    cr += 1;
                }
                texto.push(13);
            }
            10 => {
                lf += 1;
                texto.push(13);
            }
            0 => {
                nulos = true;
                texto.push(b' ' as u16);
            }
            c => texto.push(c),
        }
        i += 1;
    }
    let saltos = if crlf == 0 && lf == 0 && cr == 0 || crlf >= lf && crlf >= cr {
        Saltos::Crlf
    } else if lf >= cr {
        Saltos::Lf
    } else {
        Saltos::Cr
    };
    let mezclados = (crlf > 0) as u8 + (lf > 0) as u8 + (cr > 0) as u8 > 1;
    let aviso = if nulos {
        Aviso::Nulos
    } else if malos {
        Aviso::BytesNoValidos
    } else if mezclados {
        Aviso::Mezclados
    } else {
        Aviso::Ninguno
    };
    Leido { texto, cod, saltos, aviso, nulos }
}

/// ¿UTF-8 con pocos bytes rotos? Al menos tantas letras de varios bytes bien formadas como trozos rotos. Un texto ANSI
/// con tildes casi nunca forma una: cada letra es un byte suelto que no vale en UTF-8.
fn casi_utf8(b: &[u8]) -> bool {
    let (mut buenas, mut rotos) = (0usize, 0usize);
    for t in b.utf8_chunks() {
        buenas += t.valid().chars().filter(|c| c.len_utf8() > 1).count();
        rotos += !t.invalid().is_empty() as usize;
    }
    buenas > 0 && rotos <= buenas
}

// UTF-16 sin marca de inicio: con letras latinas, uno de cada dos bytes es cero y siempre el mismo.
pub fn parece_utf16(b: &[u8]) -> Option<bool> {
    if b.len() < 4 || !b.len().is_multiple_of(2) {
        return None;
    }
    let pares = b.len() / 2;
    let ceros_pares = b.iter().step_by(2).filter(|&&x| x == 0).count();
    let ceros_impares = b.iter().skip(1).step_by(2).filter(|&&x| x == 0).count();
    if ceros_pares == 0 && ceros_impares * 10 >= pares * 3 {
        Some(false)
    } else if ceros_impares == 0 && ceros_pares * 10 >= pares * 3 {
        Some(true)
    } else {
        None
    }
}

fn desde_utf16(b: &[u8], be: bool, malos: &mut bool) -> Vec<u16> {
    *malos |= !b.len().is_multiple_of(2);
    let t: Vec<u16> = b
        .as_chunks::<2>().0.iter()
        .map(|p| if be { u16::from_be_bytes([p[0], p[1]]) } else { u16::from_le_bytes([p[0], p[1]]) })
        .collect();
    *malos |= char::decode_utf16(t.iter().copied()).any(|c| c.is_err());
    t
}

fn desde_ansi(b: &[u8], cp: u32, malos: &mut bool) -> Vec<u16> {
    if b.is_empty() {
        return Vec::new();
    }
    unsafe {
        // Primero sin admitir secuencias rotas (p. ej., media letra japonesa al final): si las hay, se avisa y al
        // guardar se pide confirmar, porque esos bytes no vuelven a quedar como estaban.
        let mut flags = MB_ERR_INVALID_CHARS;
        let mut n = MultiByteToWideChar(cp, flags, b.as_ptr(), b.len() as i32, std::ptr::null_mut(), 0);
        if n == 0 {
            *malos = true;
            flags = 0;
            n = MultiByteToWideChar(cp, flags, b.as_ptr(), b.len() as i32, std::ptr::null_mut(), 0);
        }
        let mut t = vec![0u16; n.max(0) as usize];
        let n = MultiByteToWideChar(cp, flags, b.as_ptr(), b.len() as i32, t.as_mut_ptr(), n);
        t.truncate(n.max(0) as usize);
        t
    }
}

/// Lo pegado o soltado, con cualquier tipo de salto, a saltos de Bloc.
pub fn normalizar(t: &[u16]) -> Vec<u16> {
    let mut r = Vec::with_capacity(t.len());
    let mut i = 0;
    while i < t.len() {
        match t[i] {
            13 if t.get(i + 1) == Some(&10) => {
                r.push(13);
                i += 1;
            }
            10 => r.push(13),
            0 => r.push(b' ' as u16),
            c => r.push(c),
        }
        i += 1;
    }
    r
}

/// El texto de Bloc con los saltos que se piden.
pub fn con_saltos(t: &[u16], saltos: Saltos) -> Vec<u16> {
    match saltos {
        Saltos::Cr => t.to_vec(),
        Saltos::Lf => t.iter().map(|&c| if c == 13 { 10 } else { c }).collect(),
        Saltos::Crlf => {
            let mut r = Vec::with_capacity(t.len() + t.len() / 16);
            for &c in t {
                r.push(c);
                if c == 13 {
                    r.push(10);
                }
            }
            r
        }
    }
}

/// Bytes del archivo (marca de inicio y cuerpo) y la codificación usada: si el texto no cabe en ANSI pasa a UTF-8
/// antes que perder caracteres.
pub fn codificar(texto: &[u16], cod: Cod, saltos: Saltos) -> (Vec<u8>, Vec<u8>, Cod) {
    let t = con_saltos(texto, saltos);
    match cod {
        Cod::Utf8 | Cod::Utf8Bom => {
            let s = String::from_utf16_lossy(&t);
            let bom = if cod == Cod::Utf8Bom { vec![0xEF, 0xBB, 0xBF] } else { Vec::new() };
            (bom, s.into_bytes(), cod)
        }
        Cod::Utf16 { be, bom } => {
            let marca = match (bom, be) {
                (false, _) => Vec::new(),
                (true, false) => vec![0xFF, 0xFE],
                (true, true) => vec![0xFE, 0xFF],
            };
            let cuerpo = t.iter().flat_map(|c| if be { c.to_be_bytes() } else { c.to_le_bytes() }).collect();
            (marca, cuerpo, cod)
        }
        Cod::Ansi(cp) => match a_ansi(&t, cp) {
            Some(b) => (Vec::new(), b, cod),
            None => (Vec::new(), String::from_utf16_lossy(&t).into_bytes(), Cod::Utf8),
        },
    }
}

fn a_ansi(t: &[u16], cp: u32) -> Option<Vec<u8>> {
    if t.is_empty() {
        return Some(Vec::new());
    }
    const WC_NO_BEST_FIT_CHARS: u32 = 0x400;
    unsafe {
        let mut usado = 0i32;
        // Con «mejor parecido» Windows cambiaría ă por a sin decir nada; hay codificaciones que no admiten la opción.
        let mut opciones = WC_NO_BEST_FIT_CHARS;
        let mut n = WideCharToMultiByte(cp, opciones, t.as_ptr(), t.len() as i32, std::ptr::null_mut(), 0, std::ptr::null(), &mut usado);
        if n == 0 {
            opciones = 0;
            n = WideCharToMultiByte(cp, 0, t.as_ptr(), t.len() as i32, std::ptr::null_mut(), 0, std::ptr::null(), &mut usado);
        }
        if n <= 0 || usado != 0 {
            return None;
        }
        let mut b = vec![0u8; n as usize];
        let n = WideCharToMultiByte(cp, opciones, t.as_ptr(), t.len() as i32, b.as_mut_ptr(), n, std::ptr::null(), &mut usado);
        if n <= 0 || usado != 0 { None } else { Some(b) }
    }
}

fn ancho(p: &Path) -> Vec<u16> {
    p.as_os_str().encode_wide().chain(std::iter::once(0)).collect()
}

/// Escribe un archivo que no existe: si mientras tanto lo ha creado otro, falla en vez de vaciarlo.
fn volcar(ruta: &Path, bom: &[u8], cuerpo: &[u8]) -> io::Result<()> {
    let mut f = OpenOptions::new().write(true).create_new(true).share_mode(0).open(ruta)?;
    f.write_all(bom)?;
    f.write_all(cuerpo)?;
    f.sync_all() // en disco antes de tocar el original
}

fn borrar(ruta: &Path) {
    let _ = fs::remove_file(ruta);
}

/// Guarda sin dejar nunca el original a medias. Devuelve la codificación usada.
pub fn escribir(ruta: &Path, texto: &[u16], cod: Cod, saltos: Saltos) -> Result<Cod, ErrorGuardar> {
    // Un enlace simbólico (mklink) se guarda en el archivo al que lleva: cambiar o sustituir el enlace lo rompería y el
    // archivo de verdad se quedaría sin los cambios.
    let real;
    let ruta = match fs::symlink_metadata(ruta) {
        Ok(m) if m.file_type().is_symlink() => {
            real = fs::canonicalize(ruta).map_err(ErrorGuardar::Io)?;
            real.as_path()
        }
        _ => ruta,
    };
    let (bom, cuerpo, usada) = codificar(texto, cod, saltos);
    let meta = match fs::metadata(ruta) {
        Ok(m) => m,
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            return volcar(ruta, &bom, &cuerpo).map(|_| usada).map_err(ErrorGuardar::Io);
        }
        Err(e) => return Err(ErrorGuardar::Io(e)),
    };
    if meta.permissions().readonly() {
        return Err(ErrorGuardar::SoloLectura);
    }

    // Primero a un temporal y luego se cambia por el original: si algo falla a medias, el original queda entero.
    let azar = std::collections::hash_map::RandomState::new().build_hasher().finish() as u32;
    let mut marca = ruta.as_os_str().to_owned();
    marca.push(format!(".{azar:08x}"));
    let con = |fin: &str| {
        let mut x = marca.clone();
        x.push(fin);
        PathBuf::from(x)
    };
    let tmp = con(".bloc-tmp");
    if let Err(e) = volcar(&tmp, &bom, &cuerpo) {
        borrar(&tmp);
        return Err(ErrorGuardar::Io(e));
    }
    let (r, t) = (ancho(ruta), ancho(&tmp));
    // REPLACEFILE_IGNORE_MERGE_ERRORS | REPLACEFILE_IGNORE_ACL_ERRORS
    if unsafe { ReplaceFileW(r.as_ptr(), t.as_ptr(), std::ptr::null(), 0x2 | 0x4, std::ptr::null(), std::ptr::null()) } != 0 {
        return Ok(usada);
    }
    // Hay unidades de red que no admiten el reemplazo, y un fallo del reemplazo que ya ha borrado el original:
    // entonces solo queda poner el nuevo en su sitio y, si tampoco se puede, decir dónde ha quedado.
    if !ruta.exists() {
        return fs::rename(&tmp, ruta)
            .map(|_| usada)
            .map_err(|e| ErrorGuardar::Io(io::Error::new(e.kind(), format!("{e}\n\n{}", tmp.display()))));
    }

    // Sin reemplazo: se aparta el original, se pone el nuevo y solo entonces se borra el apartado.
    // Son cambios de nombre: ninguno deja un archivo a medias.
    let apartado = con(".bloc-original");
    let atributos = unsafe { GetFileAttributesW(r.as_ptr()) };
    let creado = meta.created().ok();
    if let Err(e) = fs::rename(ruta, &apartado) {
        borrar(&tmp);
        return Err(ErrorGuardar::Io(e));
    }
    if let Err(e) = fs::rename(&tmp, ruta) {
        if fs::rename(&apartado, ruta).is_err() {
            return Err(ErrorGuardar::Apartado { original: apartado, nuevo: tmp });
        }
        borrar(&tmp);
        return Err(ErrorGuardar::Io(e));
    }
    if atributos != INVALID_FILE_ATTRIBUTES {
        unsafe { SetFileAttributesW(r.as_ptr(), atributos) };
    }
    if let (Some(c), Ok(f)) = (creado, OpenOptions::new().write(true).open(ruta)) {
        let _ = f.set_times(fs::FileTimes::new().set_created(c));
    }
    borrar(&apartado);
    Ok(usada)
}

#[cfg(test)]
mod pruebas {
    use super::*;

    fn u(s: &str) -> Vec<u16> {
        s.encode_utf16().collect()
    }

    fn carpeta() -> PathBuf {
        let azar = std::collections::hash_map::RandomState::new().build_hasher().finish();
        let d = std::env::temp_dir().join(format!("bloc-pruebas-{azar:x}"));
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn guarda_a_traves_de_un_enlace() {
        let d = carpeta();
        let (real, enlace) = (d.join("real.txt"), d.join("enlace.txt"));
        fs::write(&real, "a").unwrap();
        // Crear un enlace pide permiso de administrador (o el modo de programador): sin él, la prueba no se puede hacer.
        if std::os::windows::fs::symlink_file(&real, &enlace).is_err() {
            let _ = fs::remove_dir_all(&d);
            return;
        }
        escribir(&enlace, &u("b"), Cod::Utf8, Saltos::Crlf).unwrap();
        assert!(fs::symlink_metadata(&enlace).unwrap().file_type().is_symlink(), "el enlace sigue siendo un enlace");
        assert_eq!(fs::read(&real).unwrap(), b"b", "lo guardado está en el archivo de verdad");
        let _ = fs::remove_dir_all(&d);
    }

    fn restos(d: &Path) -> usize {
        fs::read_dir(d).unwrap().filter(|e| e.as_ref().unwrap().file_name().to_string_lossy().contains(".bloc-")).count()
    }

    #[test]
    fn codificaciones_y_saltos() {
        let d = carpeta();
        let f = d.join("prueba.txt");

        fs::write(&f, "año\nñu\n").unwrap();
        let l = leer(&f).unwrap();
        assert!(l.texto == u("año\rñu\r") && l.saltos == Saltos::Lf && l.cod == Cod::Utf8, "UTF-8 sin BOM con LF");
        escribir(&f, &[l.texto.clone(), u("más")].concat(), l.cod, l.saltos).unwrap();
        assert_eq!(fs::read(&f).unwrap(), "año\nñu\nmás".as_bytes(), "guarda igual: UTF-8 sin BOM y LF");

        fs::write(&f, [&[0xEF, 0xBB, 0xBF][..], b"uno\r\ndos"].concat()).unwrap();
        let l = leer(&f).unwrap();
        assert!(l.texto == u("uno\rdos") && l.saltos == Saltos::Crlf && l.cod == Cod::Utf8Bom, "UTF-8 con BOM y CRLF");
        escribir(&f, &l.texto, l.cod, l.saltos).unwrap();
        let b = fs::read(&f).unwrap();
        assert!(b.len() == 11 && b[0] == 0xEF && b[3] == b'u', "guarda el BOM");

        if ansi() == 1252 {
            fs::write(&f, b"canci\xf3n\r\n").unwrap();
            let l = leer(&f).unwrap();
            assert!(l.texto == u("canción\r") && l.cod == Cod::Ansi(1252), "lee ANSI");
            let usada = escribir(&f, &[l.texto.clone(), u("€")].concat(), l.cod, l.saltos).unwrap();
            assert!(usada == Cod::Ansi(1252) && fs::read(&f).unwrap() == b"canci\xf3n\r\n\x80", "guarda ANSI");
            let usada = escribir(&f, &u("日本"), l.cod, l.saltos).unwrap();
            assert!(usada == Cod::Utf8 && fs::read_to_string(&f).unwrap() == "日本", "si no cabe en ANSI, UTF-8 sin perder nada");
        }

        fs::write(&f, [0xFF, 0xFE, b'h', 0, b'o', 0, b'l', 0, b'a', 0]).unwrap();
        let l = leer(&f).unwrap();
        assert!(l.texto == u("hola") && l.cod == Cod::Utf16 { be: false, bom: true }, "UTF-16");
        fs::write(&f, [b'h', 0, b'o', 0, b'l', 0, b'a', 0, 13, 0, 10, 0]).unwrap();
        let l = leer(&f).unwrap();
        assert!(l.texto == u("hola\r") && l.cod == Cod::Utf16 { be: false, bom: false }, "UTF-16 sin BOM");
        escribir(&f, &l.texto, l.cod, l.saltos).unwrap();
        assert_eq!(fs::read(&f).unwrap(), [b'h', 0, b'o', 0, b'l', 0, b'a', 0, 13, 0, 10, 0], "UTF-16 sin BOM se guarda igual");
        assert_eq!(restos(&d), 0, "no deja temporales");

        assert_eq!(decodificar(&[0xFF, 0xFE, 0x41, 0x00, 0x00, 0xD8]).aviso, Aviso::BytesNoValidos, "UTF-16 roto: avisa");
        assert_eq!(decodificar(b"a\r\nb\nc").aviso, Aviso::Mezclados, "saltos mezclados: avisa");
        let l = decodificar(b"caf\xc3\xa9 y ni\xc3\xb1o \xff fin");
        assert!(l.cod == Cod::Utf8 && l.aviso == Aviso::BytesNoValidos, "UTF-8 con un byte roto: UTF-8 y avisa");
        assert_eq!(String::from_utf16_lossy(&l.texto), "café y niño \u{FFFD} fin");
        assert!(matches!(decodificar(b"caf\xe9 y ni\xf1o").cod, Cod::Ansi(_)), "ANSI con tildes: sigue siendo ANSI");
        let l = decodificar(b"a\rb\rc");
        assert!(l.texto == u("a\rb\rc") && l.saltos == Saltos::Cr && l.aviso == Aviso::Ninguno, "Mac antiguo: se respeta");
        assert_eq!(decodificar(b"a\r\nb\r\nc").aviso, Aviso::Ninguno, "saltos de Windows: sin aviso");
        let l = decodificar(b"a\0b");
        assert!(l.nulos && l.aviso == Aviso::Nulos && l.texto == u("a b"), "nulos: avisa y se ven como espacios");
        assert_eq!(normalizar(&u("a\nb\rc\r\nd")), u("a\rb\rc\rd"), "pegar con cualquier salto");

        fs::write(&f, "intocable").unwrap();
        let mut p = fs::metadata(&f).unwrap().permissions();
        p.set_readonly(true);
        fs::set_permissions(&f, p.clone()).unwrap();
        let r = escribir(&f, &u("otro"), Cod::Utf8, Saltos::Crlf);
        #[allow(clippy::permissions_set_readonly_false)]
        p.set_readonly(false);
        fs::set_permissions(&f, p).unwrap();
        assert!(matches!(r, Err(ErrorGuardar::SoloLectura)) && fs::read_to_string(&f).unwrap() == "intocable" && restos(&d) == 0,
            "solo lectura: no se toca ni deja temporales");
        let _ = fs::remove_dir_all(&d);
    }
}
