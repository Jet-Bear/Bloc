// La nota para la próxima vez: lo que se cierra con «Seguir la próxima vez» se guarda en la carpeta de Bloc del perfil
// y se abre solo la próxima vez que se abra Bloc sin archivo. Como mucho una. No se borra al abrirla, sino al decidir
// sobre ella (guardar o no guardar): si Bloc se cierra de golpe, sigue ahí.
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::os::windows::fs::OpenOptionsExt;
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::fichero::{Cod, Saltos};

const CR: u16 = 13;

pub struct Nota {
    /// Como se guardaría en el archivo, con \r por salto.
    pub texto: Vec<u16>,
    pub ruta: Option<PathBuf>,
    pub cod: Cod,
    pub saltos: Saltos,
    /// La hora del archivo con la que se trabajaba: si al guardar es otra, alguien lo ha cambiado entretanto.
    pub escrito: Option<SystemTime>,
    /// Se ve como Markdown (cuenta sin archivo, cuando no lo dice la extensión).
    pub md: bool,
    pub sel: (usize, usize),
    /// Dónde empieza lo que se veía arriba.
    pub arriba: usize,
    pub fecha: SystemTime,
    /// Cuántas veces se ha escrito: la ventana que la tiene abierta solo la da por suya si nadie la ha cambiado después.
    pub serie: u64,
}

fn archivo(nombre: &str) -> Option<PathBuf> {
    Some(crate::config::carpeta()?.join(nombre))
}

fn nanos(t: SystemTime) -> u64 {
    t.duration_since(UNIX_EPOCH).map_or(0, |d| d.as_nanos() as u64)
}

fn de_nanos(n: u64) -> SystemTime {
    UNIX_EPOCH + Duration::from_nanos(n)
}

fn nombre_cod(c: Cod) -> String {
    match c {
        Cod::Utf8 => "utf8".into(),
        Cod::Utf8Bom => "utf8bom".into(),
        Cod::Utf16 { be, bom } => format!("utf16{}{}", if be { "be" } else { "le" }, if bom { "" } else { "-sinbom" }),
        Cod::Ansi(cp) => format!("ansi{cp}"),
    }
}

fn cod_de(s: &str) -> Option<Cod> {
    Some(match s {
        "utf8" => Cod::Utf8,
        "utf8bom" => Cod::Utf8Bom,
        _ if s.starts_with("utf16") => Cod::Utf16 { be: s[5..].starts_with("be"), bom: !s.ends_with("-sinbom") },
        _ => Cod::Ansi(s.strip_prefix("ansi")?.parse().ok()?),
    })
}

fn saltos_de(s: &str) -> Option<Saltos> {
    Some(match s {
        "crlf" => Saltos::Crlf,
        "lf" => Saltos::Lf,
        "cr" => Saltos::Cr,
        _ => return None,
    })
}

impl Nota {
    fn bytes(&self) -> Vec<u8> {
        let saltos = match self.saltos {
            Saltos::Crlf => "crlf",
            Saltos::Lf => "lf",
            Saltos::Cr => "cr",
        };
        let ruta = self.ruta.as_ref().and_then(|r| r.to_str()).unwrap_or("");
        let escrito = self.escrito.map_or(String::new(), |e| nanos(e).to_string());
        let mut b = format!(
            "bloc-nota 1\nruta={ruta}\ncod={}\nsaltos={saltos}\nescrito={escrito}\nmd={}\nsel={},{}\narriba={}\nfecha={}\nserie={}\nlargo={}\n\n",
            nombre_cod(self.cod),
            self.md as u8,
            self.sel.0,
            self.sel.1,
            self.arriba,
            nanos(self.fecha),
            self.serie,
            self.texto.len()
        )
        .into_bytes();
        b.extend(self.texto.iter().flat_map(|c| c.to_le_bytes()));
        b
    }

    fn de_bytes(b: &[u8]) -> Option<Nota> {
        let corte = b.windows(2).position(|w| w == b"\n\n")?;
        let cabeza = std::str::from_utf8(&b[..corte]).ok()?;
        let mut lineas = cabeza.lines();
        if lineas.next()? != "bloc-nota 1" {
            return None;
        }
        let campos: std::collections::HashMap<&str, &str> = lineas.filter_map(|l| l.split_once('=')).collect();
        let campo = |k: &str| campos.get(k).copied();
        let numero = |k: &str| campo(k)?.parse::<u64>().ok();
        let cuerpo = &b[corte + 2..];
        let largo = numero("largo")? as usize;
        if cuerpo.len() != largo * 2 {
            return None; // a medias: no es una nota buena
        }
        let (a, z) = campo("sel")?.split_once(',')?;
        Some(Nota {
            texto: cuerpo.chunks(2).map(|p| u16::from_le_bytes([p[0], p[1]])).collect(),
            ruta: campo("ruta").filter(|r| !r.is_empty()).map(PathBuf::from),
            cod: cod_de(campo("cod")?)?,
            saltos: saltos_de(campo("saltos")?)?,
            escrito: numero("escrito").map(de_nanos),
            md: campo("md")? == "1",
            sel: (a.parse().ok()?, z.parse().ok()?),
            arriba: numero("arriba")? as usize,
            fecha: de_nanos(numero("fecha")?),
            serie: numero("serie")?,
        })
    }
}

pub fn leer() -> Option<Nota> {
    leer_como("nota.bloc")
}

/// Una nota con otro nombre en la carpeta de Bloc (las copias mientras se escribe, ver copia.rs).
pub fn leer_como(nombre: &str) -> Option<Nota> {
    Nota::de_bytes(&fs::read(archivo(nombre)?).ok()?)
}

pub fn escribir(n: &Nota) -> io::Result<()> {
    escribir_como("nota.bloc", n)
}

/// Primero entera en otro archivo y en el disco; después, en lugar de la que hubiera: un apagón a medias no deja una
/// nota rota ni se lleva la anterior.
pub fn escribir_como(nombre: &str, n: &Nota) -> io::Result<()> {
    let destino = archivo(nombre).ok_or_else(|| io::Error::from(io::ErrorKind::NotFound))?;
    if let Some(d) = destino.parent() {
        fs::create_dir_all(d)?;
    }
    let tmp = destino.with_extension(format!("bloc.{}.nuevo", std::process::id()));
    let r = (|| {
        let mut f = File::create(&tmp)?;
        f.write_all(&n.bytes())?;
        f.sync_all()?;
        drop(f);
        fs::rename(&tmp, &destino)
    })();
    if r.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    r
}

pub fn borrar() {
    borrar_como("nota.bloc");
}

pub fn borrar_como(nombre: &str) {
    if let Some(a) = archivo(nombre) {
        let _ = fs::remove_file(a);
    }
}

/// Un archivo abierto solo por quien lo tiene; se va solo al cerrarse, también si Bloc acaba de golpe.
pub fn exclusivo(nombre: &str) -> io::Result<File> {
    let r = archivo(nombre).ok_or_else(|| io::Error::from(io::ErrorKind::NotFound))?;
    if let Some(d) = r.parent() {
        fs::create_dir_all(d)?;
    }
    OpenOptions::new().write(true).create(true).truncate(true).share_mode(0).custom_flags(0x0400_0000).open(r) // FILE_FLAG_DELETE_ON_CLOSE
}

/// Mientras una ventana lee y escribe la nota: dos que cierran a la vez no se pisan. Espera a la otra hasta 2 s.
pub struct Cerrojo(#[allow(dead_code)] File);

pub fn cerrojo() -> Option<Cerrojo> {
    let fin = Instant::now() + Duration::from_secs(2);
    loop {
        match exclusivo("nota.cerrojo") {
            Ok(f) => return Some(Cerrojo(f)),
            // 5: el del otro proceso, aún pendiente de borrar.
            Err(e) if matches!(e.raw_os_error(), Some(5 | 32)) && Instant::now() < fin => std::thread::sleep(Duration::from_millis(20)),
            Err(_) => return None,
        }
    }
}

/// La tiene la ventana que la enseña: otra que se abra mientras, sale vacía.
pub struct Tenencia(#[allow(dead_code)] File);

pub fn tomar() -> Option<Tenencia> {
    exclusivo("nota.abierta").ok().map(Tenencia)
}

/// Hay nota y nadie la tiene abierta.
pub fn libre() -> bool {
    archivo("nota.bloc").is_some_and(|a| a.exists()) && tomar().is_some()
}

/// `vieja` sin sus líneas vacías del final, 5 líneas vacías, una barra de 40 «=», una línea vacía y `nueva`. Si la
/// vieja no tiene nada, solo la nueva.
pub fn unir(vieja: &[u16], nueva: &[u16]) -> Vec<u16> {
    let blanco = |c: &u16| *c == CR || *c == b' ' as u16 || *c == b'\t' as u16;
    let Some(ultima) = vieja.iter().rposition(|c| !blanco(c)) else { return nueva.to_vec() };
    // Hasta el final de su última línea con algo: sus espacios de detrás se quedan.
    let fin = vieja[ultima..].iter().position(|&c| c == CR).map_or(vieja.len(), |k| ultima + k);
    let mut r = vieja[..fin].to_vec();
    r.extend([CR; 6]);
    r.extend([b'=' as u16; 40]);
    r.extend([CR, CR]);
    r.extend_from_slice(nueva);
    r
}

/// `nueva` al final de `vieja` (ver `unir`): «Sin título», para que Ctrl+S no escriba una nota dentro del archivo de la
/// otra, y como se veía la vieja. El cursor, en lo añadido, y se ve desde la barra que las separa.
pub fn anadir(vieja: &Nota, mut nueva: Nota) -> Nota {
    let largo = nueva.texto.len();
    nueva.texto = unir(&vieja.texto, &nueva.texto);
    let desde = nueva.texto.len() - largo;
    nueva.sel = (desde + nueva.sel.0, desde + nueva.sel.1);
    nueva.arriba = desde.saturating_sub(42);
    (nueva.ruta, nueva.cod, nueva.saltos, nueva.escrito, nueva.md) = (None, vieja.cod, vieja.saltos, None, vieja.md);
    nueva.serie = vieja.serie + 1;
    nueva
}

/// La primera línea con algo, para el aviso de «Ya hay una nota»; las largas, cortadas.
pub fn primera_linea(t: &[u16]) -> String {
    let l = t.split(|&c| c == CR).map(String::from_utf16_lossy).find(|l| !l.trim().is_empty()).unwrap_or_default();
    let l = l.trim();
    if l.chars().count() > 80 { format!("{}…", l.chars().take(80).collect::<String>()) } else { l.to_string() }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    fn w(s: &str) -> Vec<u16> {
        s.encode_utf16().collect()
    }

    #[test]
    fn unir_con_la_barra() {
        let barra = "=".repeat(40);
        assert_eq!(unir(&w("uno\rdos  \r\r  \r"), &w("tres")), w(&format!("uno\rdos  \r\r\r\r\r\r{barra}\r\rtres")));
        assert_eq!(unir(&w("\r \r"), &w("tres")), w("tres"), "sin nada antes, solo la nueva");
    }

    #[test]
    fn de_ida_y_vuelta() {
        let n = Nota {
            texto: [w("hola\r\u{2005}"), vec![0xD800], w("fin")].concat(),
            ruta: Some(PathBuf::from(r"A:\x=1\nota.md")),
            cod: Cod::Utf16 { be: true, bom: false },
            saltos: Saltos::Lf,
            escrito: Some(UNIX_EPOCH + Duration::from_nanos(1_700_000_000_123_456_700)),
            md: true,
            sel: (2, 4),
            arriba: 1,
            fecha: UNIX_EPOCH + Duration::from_secs(1_800_000_000),
            serie: 7,
        };
        let mut corta = n.bytes();
        let m = Nota::de_bytes(&corta).expect("se lee");
        assert_eq!((m.texto, m.ruta, m.cod, m.saltos, m.escrito, m.md, m.sel, m.arriba, m.fecha, m.serie), (n.texto, n.ruta, n.cod, n.saltos, n.escrito, n.md, n.sel, n.arriba, n.fecha, n.serie));
        corta.pop();
        assert!(Nota::de_bytes(&corta).is_none(), "a medias no vale");
    }
}
