// Las tablas de los Markdown, como tablas: rejilla, cabecera en negrita y celdas que parten su texto. El control tiene
// cada fila plegada al alto que le toca y con sus letras sin verse (md::Parrafo::alto), y Bloc la dibuja encima tras
// cada WM_PAINT, como las rayas. El texto no cambia: copiar, buscar, deshacer y guardar no se enteran. Aquí, cuánto
// mide cada columna y cada fila, y el dibujo.
use std::cell::RefCell;
use std::collections::HashMap;

use windows_sys::Win32::Foundation::{RECT, SIZE};
use windows_sys::Win32::Graphics::Gdi::*;

use crate::md;

/// Lo que cambia el diseño de una tabla: el zoom, los puntos por pulgada y el ancho del texto (0, sin ajuste de línea).
pub type Clave = (i32, i32, i32);

thread_local! {
    // Las letras de las celdas, para un zoom y unos puntos por pulgada: una por combinación de negrita, cursiva,
    // tachado, código y enlace.
    static LETRAS: RefCell<((i32, i32), HashMap<u8, HFONT>)> = RefCell::new(((0, 0), HashMap::new()));
}

fn letra(zoom: i32, dpi: i32, bits: u8) -> HFONT {
    LETRAS.with(|l| {
        let mut l = l.borrow_mut();
        if l.0 != (zoom, dpi) {
            for &f in l.1.values() {
                unsafe { DeleteObject(f) };
            }
            l.1.clear();
            l.0 = (zoom, dpi);
        }
        *l.1.entry(bits).or_insert_with(|| crear_letra(zoom, dpi, bits))
    })
}

/// La letra que pone RichEdit con ese formato (ver editor::letra), al tamaño de este zoom.
fn crear_letra(zoom: i32, dpi: i32, bits: u8) -> HFONT {
    let mono = bits & md::CODIGO != 0;
    let medios = if mono { md::TAM_CODIGO } else { md::TAM } as i32;
    let mut f: LOGFONTW = unsafe { std::mem::zeroed() };
    f.lfHeight = -((medios * dpi * zoom + 7200) / 14400);
    // La negrita de la vista previa es seminegrita: Segoe UI Semibold. Consolas no la tiene: negrita.
    let semi = !mono && bits & md::NEGRITA != 0;
    f.lfWeight = if mono && bits & md::NEGRITA != 0 { 700 } else { 400 };
    f.lfItalic = (bits & md::CURSIVA != 0) as u8;
    f.lfUnderline = (bits & md::ENLACE != 0) as u8;
    f.lfStrikeOut = (bits & md::TACHADO != 0) as u8;
    f.lfCharSet = DEFAULT_CHARSET;
    for (i, c) in if mono { "Consolas" } else if semi { "Segoe UI Semibold" } else { "Segoe UI" }.encode_utf16().enumerate() {
        f.lfFaceName[i] = c;
    }
    unsafe { CreateFontIndirectW(&f) }
}

/// Los twips de interlineado exacto que RichEdit pinta como `px` píxeles con ese zoom: por arriba, para que su
/// redondeo, sea hacia abajo o al más cercano, dé justo `px`.
pub fn twips(px: i32, zoom: i32, dpi: i32) -> i32 {
    let d = (dpi * zoom) as i64;
    ((px as i64 * 144_000 + d - 1) / d) as i32
}

/// Un trozo de una línea de una celda: dónde empieza (desde el principio de su línea), cuánto mide y con qué letra.
struct Trozo {
    x: i32,
    ancho: i32,
    texto: Vec<u16>,
    letra: u8,
}

/// Una palabra: sus trozos de la misma letra y lo que mide.
struct Palabra {
    trozos: Vec<(Vec<u16>, u8)>,
    ancho: i32,
}

pub struct Diseno {
    /// El ancho de cada columna, con la raya de su izquierda.
    anchos: Vec<i32>,
    /// El alto de cada línea de la tabla: la cabecera, con la raya de arriba; la |---|, que es la raya bajo la cabecera;
    /// y cada fila, con la raya de debajo.
    pub altos: Vec<i32>,
    /// Cada fila, sus celdas, y cada celda sus líneas.
    celdas: Vec<Vec<Vec<Vec<Trozo>>>>,
    alinear: Vec<u8>,
    /// El renglón (1,5 veces la letra), dónde cae la línea base en él y lo que mide la letra.
    linea: i32,
    ascenso: i32,
    alto_letra: i32,
    hueco: (i32, i32),
    /// El zoom y los puntos por pulgada de sus letras.
    letras: (i32, i32),
}

impl Diseno {
    pub fn ancho(&self) -> i32 {
        self.anchos.iter().sum::<i32>() + 1
    }

    /// La fila y la columna del punto (x, y), contado desde la esquina de la tabla. La |---| cuenta como la cabecera.
    pub fn celda(&self, x: i32, y: i32) -> Option<(usize, usize)> {
        if x < 0 || y < 0 || x >= self.ancho() {
            return None;
        }
        let (mut arriba, mut fila) = (0, None);
        for (k, &h) in self.altos.iter().enumerate() {
            if y < arriba + h {
                fila = Some(if k == 1 { 0 } else { k });
                break;
            }
            arriba += h;
        }
        let mut izquierda = 0;
        let columna = self.anchos.iter().position(|&w| {
            izquierda += w;
            x < izquierda
        });
        Some((fila?, columna.unwrap_or(self.anchos.len() - 1)))
    }

}

/// Las palabras de una celda, cada una en trozos de la misma letra. `extra`, lo que llevan todas (la cabecera, negrita).
fn palabras(celda: &[(u16, u8)], extra: u8) -> Vec<Vec<(Vec<u16>, u8)>> {
    let mut r: Vec<Vec<(Vec<u16>, u8)>> = Vec::new();
    let mut nueva = true;
    for &(c, f) in celda {
        if md::espacio(c) {
            nueva = true;
            continue;
        }
        if nueva {
            r.push(Vec::new());
            nueva = false;
        }
        let p = r.last_mut().expect("recién puesta");
        match p.last_mut() {
            Some((t, b)) if *b == f | extra => t.push(c),
            _ => p.push((vec![c], f | extra)),
        }
    }
    r
}

/// El ancho de cada columna en `total` píxeles, como reparten los navegadores una tabla de HTML: a su ancho natural si
/// caben todas; si no, cada una a lo que mide su palabra más larga (`palabra`) y lo que sobra, en proporción a lo que
/// le falta para su ancho natural. Si ni las palabras caben, en proporción a ellas. Nunca por debajo de `minimo`.
fn repartir(natural: &[i32], palabra: &[i32], total: i32, minimo: i32) -> Vec<i32> {
    if natural.iter().sum::<i32>() <= total {
        return natural.to_vec();
    }
    let justo: Vec<i32> = natural.iter().zip(palabra).map(|(&n, &p)| p.max(minimo).min(n)).collect();
    let (suma_n, suma_j) = (natural.iter().sum::<i32>() as i64, justo.iter().sum::<i32>() as i64);
    let mut r: Vec<i32> = if suma_j >= total as i64 {
        justo.iter().zip(natural).map(|(&j, &n)| ((j as i64 * total as i64 / suma_j) as i32).max(minimo.min(n))).collect()
    } else {
        let sobra = total as i64 - suma_j;
        justo.iter().zip(natural).map(|(&j, &n)| j + ((n - j) as i64 * sobra / (suma_n - suma_j)) as i32).collect()
    };
    // Lo que sobra del redondeo, un píxel a cada columna recortada.
    let mut sobra = total - r.iter().sum::<i32>();
    for (x, &n) in r.iter_mut().zip(natural) {
        if sobra > 0 && *x < n {
            *x += 1;
            sobra -= 1;
        }
    }
    r
}

/// Cuánto mide cada columna y cada fila de la tabla de esas líneas (la segunda, la |---|) con ese zoom, en `ancho`
/// píxeles (0, sin límite: sin ajuste de línea).
pub fn disenar(filas: &[&[u16]], ancho: i32, zoom: i32, dpi: i32) -> Diseno {
    let px = |n: i32| (n * dpi * zoom / 9600).max(1);
    // La hoja de la vista previa va en píxeles sobre su letra de 16: aquí, sobre la del texto.
    let letra_px = (md::TAM as i32 * dpi * zoom + 7200) / 14400;
    let em = |n: i32| (letra_px * n + 8) / 16;
    let hueco = (em(13), em(6));
    let linea = (letra_px * 3 + 1) / 2;
    let alinear = md::alineacion(filas.get(1).copied().unwrap_or(&[]));
    let vistas: Vec<Vec<Vec<(u16, u8)>>> = filas.iter().enumerate().map(|(k, f)| if k == 1 { Vec::new() } else { md::celdas_vistas(f) }).collect();
    let nc = vistas.iter().map(Vec::len).chain([alinear.len(), 1]).max().unwrap_or(1);
    unsafe {
        let dc = CreateCompatibleDC(std::ptr::null_mut());
        let antes = SelectObject(dc, letra(zoom, dpi, 0));
        let mut tm: TEXTMETRICW = std::mem::zeroed();
        GetTextMetricsW(dc, &mut tm);
        let medir = |t: &[u16], bits: u8| {
            SelectObject(dc, letra(zoom, dpi, bits));
            let mut s = SIZE { cx: 0, cy: 0 };
            GetTextExtentPoint32W(dc, t.as_ptr(), t.len() as i32, &mut s);
            s.cx
        };
        // Cuántas letras de t caben en `max` píxeles (al menos una si `una`) y lo que miden.
        let cabe = |t: &[u16], bits: u8, max: i32, una: bool| {
            SelectObject(dc, letra(zoom, dpi, bits));
            let (mut n, mut s) = (0, SIZE { cx: 0, cy: 0 });
            GetTextExtentExPointW(dc, t.as_ptr(), t.len() as i32, max.max(0), &mut n, std::ptr::null_mut(), &mut s);
            let n = if n == 0 && una { 1 } else { n as usize };
            (n, if n == t.len() { s.cx } else { medir(&t[..n], bits) })
        };
        // De cada fila, el ancho de un espacio y las palabras de cada celda; la cabecera, en negrita.
        let mut espacios = Vec::new();
        let celdas: Vec<Vec<Vec<Palabra>>> = vistas
            .iter()
            .enumerate()
            .map(|(k, fila)| {
                let extra = if k == 0 { md::NEGRITA } else { 0 };
                espacios.push(medir(&[32], extra));
                (0..nc)
                    .map(|c| {
                        let ps = fila.get(c).map_or(Vec::new(), |x| palabras(x, extra));
                        ps.into_iter().map(|trozos| Palabra { ancho: trozos.iter().map(|(t, b)| medir(t, *b)).sum(), trozos }).collect()
                    })
                    .collect()
            })
            .collect();
        let natural: Vec<i32> = (0..nc)
            .map(|c| {
                let de = |(k, f): (usize, &Vec<Vec<Palabra>>)| {
                    let ps = &f[c];
                    ps.iter().map(|p| p.ancho).sum::<i32>() + espacios[k] * (ps.len() as i32 - 1).max(0)
                };
                celdas.iter().enumerate().map(de).max().unwrap_or(0).max(px(16))
            })
            .collect();
        let palabra: Vec<i32> = (0..nc).map(|c| celdas.iter().flat_map(|f| f[c].iter().map(|p| p.ancho)).max().unwrap_or(0)).collect();
        // Unos píxeles de margen: la primera letra de la fila, donde empieza la tabla, no va justo en el borde del texto.
        let cuerpo = if ancho > 0 { ancho - px(4) - (nc as i32 + 1) - nc as i32 * 2 * hueco.0 } else { i32::MAX };
        let anchos = repartir(&natural, &palabra, cuerpo, px(40));
        let partir = |ps: &[Palabra], w: i32, sp: i32| {
            let mut r: Vec<Vec<Trozo>> = vec![Vec::new()];
            let mut x = 0;
            for p in ps {
                if x > 0 && x + sp + p.ancho > w {
                    r.push(Vec::new());
                    x = 0;
                }
                if x > 0 {
                    x += sp;
                }
                // Una palabra más larga que la columna se parte donde no cabe.
                for (t, bits) in &p.trozos {
                    let mut resto: &[u16] = t;
                    while !resto.is_empty() {
                        let (n, a) = cabe(resto, *bits, w - x, x == 0);
                        if n > 0 {
                            r.last_mut().expect("nunca vacía").push(Trozo { x, ancho: a, texto: resto[..n].to_vec(), letra: *bits });
                            x += a;
                            resto = &resto[n..];
                        }
                        if !resto.is_empty() {
                            r.push(Vec::new());
                            x = 0;
                        }
                    }
                }
            }
            r
        };
        let lineas: Vec<Vec<Vec<Vec<Trozo>>>> =
            celdas.iter().enumerate().map(|(k, f)| f.iter().zip(&anchos).map(|(ps, &w)| partir(ps, w, espacios[k])).collect()).collect();
        let altos = lineas
            .iter()
            .enumerate()
            .map(|(k, f)| if k == 1 { 1 } else { f.iter().map(Vec::len).max().unwrap_or(1).max(1) as i32 * linea + 2 * hueco.1 + 1 })
            .collect();
        SelectObject(dc, antes);
        DeleteDC(dc);
        Diseno {
            anchos: anchos.iter().map(|w| w + 2 * hueco.0 + 1).collect(),
            altos,
            celdas: lineas,
            alinear,
            linea,
            ascenso: (linea - tm.tmHeight) / 2 + tm.tmAscent,
            alto_letra: tm.tmHeight,
            hueco,
            letras: (zoom, dpi),
        }
    }
}

/// El color a con `peso` partes de 255 de b.
fn mezcla(a: u32, b: u32, peso: u32) -> u32 {
    (0..3).map(|i| ((((a >> (8 * i)) & 0xFF) * (255 - peso) + ((b >> (8 * i)) & 0xFF) * peso) / 255) << (8 * i)).sum()
}

/// Con un pincel propio y no el del DC: el color de ese no queda en un metarchivo (al imprimir).
fn rellenar(dc: HDC, x: i32, y: i32, ancho: i32, alto: i32, color: u32) {
    unsafe {
        let pincel = CreateSolidBrush(color);
        FillRect(dc, &RECT { left: x, top: y, right: x + ancho, bottom: y + alto }, pincel);
        DeleteObject(pincel as _);
    }
}

/// Dibuja la tabla con su esquina en (x, y). `sel`: entera dentro de lo seleccionado; como hace RichEdit con el texto,
/// el color de la selección va encima del fondo, a un 40 %, y las letras no cambian. `filas`, solo esas (lo que cae en
/// una hoja al imprimir), con (x, y) en la esquina de la primera y raya por arriba.
pub fn dibujar(dc: HDC, d: &Diseno, x: i32, y: i32, c: &md::Colores, sel: bool, filas: Option<std::ops::Range<usize>>) {
    let enteras = filas.is_none();
    let filas = filas.unwrap_or(0..d.altos.len());
    let (ancho, alto) = (d.ancho(), d.altos[filas.clone()].iter().sum::<i32>());
    let (zoom, dpi) = d.letras;
    unsafe {
        let seleccion = GetSysColor(COLOR_HIGHLIGHT);
        let tinte = |color: u32| if sel { mezcla(color, seleccion, 102) } else { color };
        let raya = tinte(c.rejilla);
        // En pantalla, solo las filas que caen en lo que se ve: una tabla de cientos de filas se dibuja en cada tecla.
        let mut zona: RECT = std::mem::zeroed();
        if enteras {
            GetClipBox(dc, &mut zona);
        }
        let se_ve = |arriba: i32, h: i32| !enteras || arriba + h > zona.top && arriba < zona.bottom;
        rellenar(dc, x, y, ancho, alto, tinte(c.fondo));
        rellenar(dc, x, y, ancho, 1, raya);
        let mut arriba = y;
        for k in filas.clone() {
            let h = d.altos[k];
            if k > 0 && se_ve(arriba, h) {
                rellenar(dc, x, arriba + h - 1, ancho, 1, raya);
            }
            arriba += h;
        }
        let mut izquierda = x;
        for &w in &d.anchos {
            rellenar(dc, izquierda, y, 1, alto, raya);
            izquierda += w;
        }
        rellenar(dc, izquierda, y, 1, alto, raya);
        SetBkMode(dc, TRANSPARENT as i32);
        SetTextAlign(dc, TA_BASELINE);
        let antes = SelectObject(dc, letra(zoom, dpi, 0));
        let mut arriba = y;
        for (k, fila) in d.celdas.iter().enumerate().skip(filas.start).take(filas.len()) {
            if !se_ve(arriba, d.altos[k]) {
                arriba += d.altos[k];
                continue;
            }
            let mut izquierda = x;
            for (col, lineas) in fila.iter().enumerate() {
                let w = d.anchos[col];
                let cuerpo = w - 1 - 2 * d.hueco.0;
                // En medio de la fila, como en la vista previa.
                let dentro = d.altos[k] - 1 - (k == 0) as i32 - 2 * d.hueco.1;
                let medio = (dentro - lineas.len() as i32 * d.linea).max(0) / 2;
                for (i, l) in lineas.iter().enumerate() {
                    let largo = l.last().map_or(0, |t| t.x + t.ancho);
                    let dx = match d.alinear.get(col) {
                        Some(1) => (cuerpo - largo) / 2,
                        Some(2) => cuerpo - largo,
                        _ => 0,
                    }
                    .max(0);
                    let base = arriba + (k == 0) as i32 + d.hueco.1 + medio + i as i32 * d.linea + d.ascenso;
                    for t in l {
                        let tx = izquierda + 1 + d.hueco.0 + dx + t.x;
                        if t.letra & md::CODIGO != 0 {
                            let arriba_letra = base - d.ascenso + (d.linea - d.alto_letra) / 2;
                            rellenar(dc, tx, arriba_letra, t.ancho, d.alto_letra, tinte(c.fondo_codigo));
                        }
                        SelectObject(dc, letra(zoom, dpi, t.letra));
                        SetTextColor(dc, if t.letra & md::ENLACE != 0 { c.acento } else { c.texto });
                        ExtTextOutW(dc, tx, base, 0, std::ptr::null(), t.texto.as_ptr(), t.texto.len() as u32, std::ptr::null());
                    }
                }
                izquierda += w;
            }
            arriba += d.altos[k];
        }
        SelectObject(dc, antes);
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn reparte_columnas() {
        assert_eq!(repartir(&[50, 60], &[50, 30], 200, 40), vec![50, 60], "caben");
        let r = repartir(&[50, 400, 300], &[50, 100, 60], 400, 40);
        assert_eq!(r.iter().sum::<i32>(), 400, "llenan lo que hay: {r:?}");
        assert_eq!(r[0], 50, "la que cabe en una palabra no encoge");
        assert!(r[1] > r[2] && r[1] >= 100 && r[2] >= 60, "cada una según su texto: {r:?}");
        assert_eq!(repartir(&[300, 300], &[100, 100], 50, 40), vec![40, 40], "nunca menos del mínimo");
        let r = repartir(&[300, 300], &[200, 100], 240, 40);
        assert_eq!(r, vec![160, 80], "sin sitio ni para las palabras, en proporción a ellas");
    }

    #[test]
    fn twips_justos() {
        for (zoom, dpi) in [(100, 96), (110, 96), (125, 120), (200, 144), (300, 192), (90, 96)] {
            for px in 1..200 {
                let t = twips(px, zoom, dpi) as i64;
                let d = (dpi * zoom) as i64;
                assert_eq!(t * d / 144_000, px as i64, "hacia abajo, {zoom} % {dpi} ppp {px} px");
                assert_eq!((t * d + 72_000) / 144_000, px as i64, "al más cercano, {zoom} % {dpi} ppp {px} px");
            }
        }
    }

    #[test]
    fn palabras_con_letra() {
        let celda: Vec<(u16, u8)> = "ab  c".encode_utf16().map(|c| (c, if c == 'c' as u16 { md::NEGRITA } else { 0 })).collect();
        let p = palabras(&celda, 0);
        assert_eq!(p.len(), 2);
        assert_eq!(p[1][0].1, md::NEGRITA);
    }
}
