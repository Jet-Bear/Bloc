// Una página (metarchivo de GDI) dibujada con Direct2D orden a orden, el texto con DirectWrite. Direct2D, al imprimir,
// convierte en imagen cualquier metarchivo que se le dé entero; así el texto sigue siendo texto en el papel y en el
// PDF. Solo lo que escriben RichEdit y el pie de página: con cualquier otra orden, `None`, y la página va entera.
use std::collections::HashMap;
use std::ffi::c_void;

use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::System::Registry::{RegGetValueW, HKEY_LOCAL_MACHINE, RRF_RT_REG_MULTI_SZ};

use super::Graficos;
use crate::com::Com;

/// Una letra de GDI hecha de DirectWrite, con sus medidas en unidades del metarchivo.
pub struct Letra {
    clave: [u8; 92],
    cara: Com,
    em: f32,
    sube: f32,
    baja: f32,
    /// Subrayado y tachado: dónde empieza la raya bajo la línea base y su grueso, en eme.
    raya: Option<(f32, f32)>,
    tacha: Option<(f32, f32)>,
}

/// Lo que se dibuja, ya en DIP de la hoja.
pub enum Orden {
    Caja([f32; 4], u32),
    /// `texto` y `grupos` van vacíos cuando el metarchivo ya trae glifos: entonces no hay texto que dar.
    Letras { x: f32, y: f32, letra: usize, em: f32, glifos: Vec<u16>, avances: Vec<f32>, texto: Vec<u16>, grupos: Vec<u16>, color: u32 },
}

#[derive(Clone, Copy)]
struct Dc {
    texto: u32,
    fondo: u32,
    opaco: bool,
    alinear: u32,
    /// `Err(n)`: la letra de serie `n`, que solo se hace si se escribe con ella (RichEdit la elige para soltar la
    /// suya, y suele ser de mapa de bits, que DirectWrite no tiene).
    letra: Result<usize, u32>,
    pincel: Option<u32>,
    pos: (f32, f32),
}

#[derive(Clone, Copy)]
enum Objeto {
    Letra(Result<usize, u32>),
    Pincel(Option<u32>),
    Otro,
}

fn num(b: &[u8], k: usize) -> Option<i32> {
    b.get(k..k + 4).map(|x| i32::from_le_bytes([x[0], x[1], x[2], x[3]]))
}

/// Las órdenes de la página, o `None` si trae algo que aquí no se sabe dibujar igual que GDI.
pub fn leer(g: &Graficos, emf: &[u8]) -> Option<Vec<Orden>> {
    if num(emf, 0)? != 1 {
        return None;
    }
    let (px, py) = (num(emf, 72)? as f32, num(emf, 76)? as f32);
    let (mx, my) = if num(emf, 4)? >= 108 { (num(emf, 100)? as f32 / 1000.0, num(emf, 104)? as f32 / 1000.0) } else { (num(emf, 80)? as f32, num(emf, 84)? as f32) };
    // De puntos de la impresora de referencia a DIP, como coloca Direct2D el metarchivo.
    let escala = (mx * 96.0 / 25.4 / px, my * 96.0 / 25.4 / py);
    let mut dc = Dc { texto: 0, fondo: 0xFF_FFFF, opaco: true, alinear: 0, letra: Err(13), pincel: Some(0xFF_FFFF), pos: (0.0, 0.0) };
    let mut pila = Vec::new();
    let mut objetos: HashMap<u32, Objeto> = HashMap::new();
    let mut r = Vec::new();
    let mut i = 0;
    loop {
        let n = num(emf, i + 4)? as usize;
        let e = emf.get(i..i + n).filter(|_| n >= 8)?;
        let v = |k| num(e, k);
        match num(e, 0)? {
            14 => return Some(r),
            1 if i == 0 => {}
            // Recortes, comentarios y modos que no cambian nada de lo que se dibuja aquí.
            13 | 16 | 19 | 20 | 21 | 30 | 70 | 75 | 98 => {}
            17 if v(8)? == 1 => {}
            115 if v(8)? == 0 => {}
            18 => dc.opaco = v(8)? == 2,
            22 => dc.alinear = v(8)? as u32,
            24 => dc.texto = v(8)? as u32,
            25 => dc.fondo = v(8)? as u32,
            27 => dc.pos = (v(8)? as f32, v(12)? as f32),
            33 => pila.push(dc),
            34 => {
                let k = v(8)?;
                if k >= 0 {
                    return None;
                }
                for _ in 0..k.unsigned_abs() {
                    dc = pila.pop()?;
                }
            }
            37 => {
                let h = v(8)? as u32;
                match if h & 0x8000_0000 != 0 { de_reserva(h & 0x7FFF_FFFF)? } else { *objetos.get(&h)? } {
                    Objeto::Letra(l) => dc.letra = l,
                    Objeto::Pincel(c) => dc.pincel = c,
                    Objeto::Otro => {}
                }
            }
            38 | 95 => {
                objetos.insert(v(8)? as u32, Objeto::Otro);
            }
            39 => {
                let c = match v(12)? {
                    0 => Some(v(16)? as u32),
                    1 => None,
                    _ => return None,
                };
                objetos.insert(v(8)? as u32, Objeto::Pincel(c));
            }
            40 => {
                objetos.remove(&(v(8)? as u32));
            }
            76 => {
                // Solo rellenos: con una imagen de origen, la página va entera.
                if v(88)? != 0 {
                    return None;
                }
                let color = match v(40)? as u32 {
                    0x00F0_0021 => dc.pincel,
                    0x00FF_0062 => Some(0xFF_FFFF),
                    0x0000_0042 => Some(0),
                    _ => return None,
                };
                if let Some(c) = color {
                    let (x, y) = (v(24)? as f32, v(28)? as f32);
                    caja(&mut r, escala, [x, y, x + v(32)? as f32, y + v(36)? as f32], c);
                }
            }
            82 => {
                let lf: [u8; 92] = e.get(12..104)?.try_into().ok()?;
                objetos.insert(v(8)? as u32, Objeto::Letra(Ok(letra(g, lf)?)));
            }
            84 => texto(g, e, &mut dc, escala, &mut r)?,
            _ => return None,
        }
        i += n;
    }
}

fn caja(r: &mut Vec<Orden>, (sx, sy): (f32, f32), [x0, y0, x1, y1]: [f32; 4], color: u32) {
    r.push(Orden::Caja([x0 * sx, y0 * sy, x1 * sx, y1 * sy], color));
}

/// EMR_EXTTEXTOUTW: el fondo, las letras y sus rayas.
fn texto(g: &Graficos, e: &[u8], dc: &mut Dc, escala: (f32, f32), r: &mut Vec<Orden>) -> Option<()> {
    let v = |k| num(e, k);
    let (cuantos, desde, ops, de_dx) = (v(44)? as usize, v(48)? as usize, v(52)? as u32, v(72)? as usize);
    // Letras de un byte, o avances también en vertical: RichEdit no los escribe.
    if ops & (0x200 | 0x2000) != 0 || cuantos > 0xFFFF {
        return None;
    }
    if ops & 2 != 0 && ops & 0x100 == 0 {
        caja(r, escala, [v(56)? as f32, v(60)? as f32, v(64)? as f32, v(68)? as f32], dc.fondo);
    }
    if cuantos == 0 {
        return Some(());
    }
    if de_dx == 0 {
        return None;
    }
    let unidades: Vec<u16> = (0..cuantos).map(|k| e.get(desde + 2 * k..desde + 2 * k + 2).map(|b| u16::from_le_bytes([b[0], b[1]]))).collect::<Option<_>>()?;
    let pasos: Vec<f32> = (0..cuantos).map(|k| num(e, de_dx + 4 * k).map(|d| d as f32)).collect::<Option<_>>()?;
    let l = match dc.letra {
        Ok(l) => l,
        Err(n) => {
            let mut lf = [0u8; 92];
            if unsafe { GetObjectW(GetStockObject(n as i32), 92, lf.as_mut_ptr() as *mut c_void) } == 0 {
                return None;
            }
            let l = letra(g, lf)?;
            dc.letra = Ok(l);
            l
        }
    };
    let (sube, baja, rayas) = {
        let m = &g.letras.borrow()[l];
        (m.sube, m.baja, [m.raya, m.tacha].map(|r| r.map(|(pos, grueso)| (pos * m.em, grueso * m.em))))
    };
    let tramos = if ops & 0x10 != 0 { vec![Tramo { letra: l, glifos: unidades, avances: pasos, texto: Vec::new(), grupos: Vec::new() }] } else { tramos(g, l, unidades, &pasos)? };
    let ancho: f32 = tramos.iter().flat_map(|t| &t.avances).sum();
    let (rx, ry) = if dc.alinear & 1 != 0 { dc.pos } else { (v(36)? as f32, v(40)? as f32) };
    let x = match dc.alinear & 6 {
        2 => rx - ancho,
        6 => rx - ancho / 2.0,
        _ => rx,
    };
    let base = match dc.alinear & 24 {
        24 => ry,
        8 => ry - baja,
        _ => ry + sube,
    };
    if dc.alinear & 1 != 0 && dc.alinear & 6 != 6 {
        dc.pos.0 = if dc.alinear & 6 == 2 { rx - ancho } else { rx + ancho };
    }
    if dc.opaco && ops & 2 == 0 {
        caja(r, escala, [x, base - sube, x + ancho, base + baja], dc.fondo);
    }
    let (sx, sy) = escala;
    let mut xt = x;
    for t in tramos {
        let ancho_t: f32 = t.avances.iter().sum();
        let em = g.letras.borrow()[t.letra].em;
        r.push(Orden::Letras { x: xt * sx, y: base * sy, letra: t.letra, em: em * sy, glifos: t.glifos, avances: t.avances.iter().map(|a| a * sx).collect(), texto: t.texto, grupos: t.grupos, color: dc.texto });
        xt += ancho_t;
    }
    for (pos, grueso) in rayas.into_iter().flatten() {
        caja(r, escala, [x, base + pos, x + ancho, base + pos + grueso], dc.texto);
    }
    Some(())
}

/// Un trozo de texto con una sola letra.
struct Tramo {
    letra: usize,
    glifos: Vec<u16>,
    avances: Vec<f32>,
    texto: Vec<u16>,
    grupos: Vec<u16>,
}

/// Del texto a los glifos, con los avances de GDI y el mapa de grupos para que el PDF tenga el texto. Lo que la letra
/// no tiene sale, como en GDI, de la primera de sus letras enlazadas que lo tenga, en su propio tramo.
fn tramos(g: &Graficos, l: usize, unidades: Vec<u16>, pasos: &[f32]) -> Option<Vec<Tramo>> {
    let (mut puntos, mut avances, mut de_unidad) = (Vec::new(), Vec::new(), Vec::with_capacity(unidades.len()));
    let mut k = 0;
    while k < unidades.len() {
        let u = unidades[k] as u32;
        de_unidad.push(puntos.len());
        match unidades.get(k + 1).map(|&b| b as u32) {
            Some(b) if (0xD800..0xDC00).contains(&u) && (0xDC00..0xE000).contains(&b) => {
                de_unidad.push(puntos.len());
                puntos.push(0x10000 + ((u - 0xD800) << 10) + (b - 0xDC00));
                avances.push(pasos[k] + pasos[k + 1]);
                k += 2;
            }
            _ => {
                puntos.push(u);
                avances.push(pasos[k]);
                k += 1;
            }
        }
    }
    let mut glifos = indices(g, l, &puntos)?;
    let mut de = vec![l; puntos.len()];
    let faltan: Vec<usize> = (0..puntos.len()).filter(|&k| glifos[k] == 0 && puntos[k] >= 0x20).collect();
    if !faltan.is_empty() {
        let otras = enlazadas(g, l);
        for k in faltan {
            // Ni en las enlazadas: la página va entera, que GDI sabrá de dónde sacarla.
            (de[k], glifos[k]) = otras.iter().find_map(|&o| indices(g, o, &puntos[k..=k]).filter(|x| x[0] != 0).map(|x| (o, x[0])))?;
        }
    }
    let mut r = Vec::new();
    let (mut k, mut u0) = (0, 0);
    while k < puntos.len() {
        let fin = (k..puntos.len()).find(|&j| de[j] != de[k]).unwrap_or(puntos.len());
        let u1 = de_unidad.iter().position(|&p| p == fin).unwrap_or(unidades.len());
        r.push(Tramo { letra: de[k], glifos: glifos[k..fin].to_vec(), avances: avances[k..fin].to_vec(), texto: unidades[u0..u1].to_vec(), grupos: de_unidad[u0..u1].iter().map(|&p| (p - k) as u16).collect() });
        (k, u0) = (fin, u1);
    }
    Some(r)
}

fn indices(g: &Graficos, l: usize, puntos: &[u32]) -> Option<Vec<u16>> {
    let cara = &g.letras.borrow()[l].cara;
    let mut glifos = vec![0u16; puntos.len()];
    (unsafe { f!(cara, 11, fn(*const u32, u32, *mut u16) -> i32)(cara.0, puntos.as_ptr(), puntos.len() as u32, glifos.as_mut_ptr()) } >= 0).then_some(glifos)
}

/// Las letras que GDI enlaza a esta para lo que no tiene, en su orden (FontLink\SystemLink del registro).
fn enlazadas(g: &Graficos, l: usize) -> Vec<usize> {
    let clave = g.letras.borrow()[l].clave;
    let nombre: Vec<u16> = clave[28..92].chunks(2).map(|b| u16::from_le_bytes([b[0], b[1]])).take_while(|&c| c != 0).chain([0]).collect();
    let ruta = crate::w::ws(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\FontLink\SystemLink");
    let mut b = vec![0u16; 8192];
    let mut largo = (b.len() * 2) as u32;
    let leido = unsafe { RegGetValueW(HKEY_LOCAL_MACHINE, ruta.as_ptr(), nombre.as_ptr(), RRF_RT_REG_MULTI_SZ, std::ptr::null_mut(), b.as_mut_ptr().cast(), &mut largo) };
    if leido != 0 {
        return Vec::new();
    }
    let mut r = Vec::new();
    // «ARCHIVO.TTF,Nombre[,escala,escala]», una por línea; la misma letra puede venir dos veces.
    for linea in b[..largo as usize / 2].split(|&c| c == 0) {
        let Some(cara) = linea.split(|&c| c == b',' as u16).nth(1).filter(|c| !c.is_empty() && c.len() < 32) else { continue };
        let mut otra = clave;
        otra[28..92].fill(0);
        for (k, c) in cara.iter().enumerate() {
            otra[28 + 2 * k..30 + 2 * k].copy_from_slice(&c.to_le_bytes());
        }
        if let Some(o) = letra(g, otra).filter(|o| *o != l && !r.contains(o)) {
            r.push(o);
        }
    }
    r
}

/// La letra de este LOGFONTW, de la lista de `g` o hecha ahora con la que elegiría GDI.
fn letra(g: &Graficos, clave: [u8; 92]) -> Option<usize> {
    if let Some(k) = g.letras.borrow().iter().position(|l| l.clave == clave) {
        return Some(k);
    }
    let lf: LOGFONTW = unsafe { std::ptr::read_unaligned(clave.as_ptr() as *const LOGFONTW) };
    if lf.lfEscapement != 0 {
        return None;
    }
    unsafe {
        let dc = CreateCompatibleDC(std::ptr::null_mut());
        let hf = CreateFontIndirectW(&lf);
        let antes = SelectObject(dc, hf as _);
        let mut tm: TEXTMETRICW = std::mem::zeroed();
        let medida = GetTextMetricsW(dc, &mut tm) != 0;
        let mut p = std::ptr::null_mut();
        let hecho = f!(g.gdi, 6, fn(HDC, *mut *mut c_void) -> i32)(g.gdi.0, dc, &mut p);
        SelectObject(dc, antes);
        DeleteObject(hf as _);
        DeleteDC(dc);
            let cara = Com::de(hecho, p).filter(|_| medida)?;
        // DWRITE_FONT_METRICS: unidades por eme y, desde la sexta, subrayado y tachado (posición sobre la base, grueso).
        let mut d = [0u16; 10];
        f!(cara, 8, fn(*mut [u16; 10]) -> ())(cara.0, &mut d);
        let eme = d[0].max(1) as f32;
        let raya = |pos: u16, grueso: u16| (-(pos as i16 as f32) / eme, grueso as f32 / eme);
        let em = if lf.lfHeight < 0 { -lf.lfHeight } else { tm.tmHeight - tm.tmInternalLeading } as f32;
        let mut letras = g.letras.borrow_mut();
        letras.push(Letra {
            clave,
            cara,
            em,
            sube: tm.tmAscent as f32,
            baja: tm.tmDescent as f32,
            raya: (lf.lfUnderline != 0).then(|| raya(d[6], d[7])),
            tacha: (lf.lfStrikeOut != 0).then(|| raya(d[8], d[9])),
        });
        Some(letras.len() - 1)
    }
}

/// Los objetos de serie de GDI (GetStockObject).
fn de_reserva(n: u32) -> Option<Objeto> {
    Some(match n {
        0 => Objeto::Pincel(Some(0xFF_FFFF)),
        1 => Objeto::Pincel(Some(0xC0_C0C0)),
        2 => Objeto::Pincel(Some(0x80_8080)),
        3 => Objeto::Pincel(Some(0x40_4040)),
        4 => Objeto::Pincel(Some(0)),
        5 => Objeto::Pincel(None),
        6..=8 | 15 | 19 => Objeto::Otro,
        10..=17 => Objeto::Letra(Err(n)),
        _ => return None,
    })
}

#[repr(C)]
struct Corrida {
    cara: *mut c_void,
    em: f32,
    cuantos: u32,
    glifos: *const u16,
    avances: *const f32,
    desplazamientos: *const c_void,
    de_lado: i32,
    nivel: u32,
}

#[repr(C)]
struct Descripcion {
    idioma: *const u16,
    texto: *const u16,
    largo: u32,
    grupos: *const u16,
    posicion: u32,
}

#[repr(C)]
struct Punto(f32, f32);

/// Las órdenes, entre BeginDraw y EndDraw. Devuelve los pinceles, que tienen que vivir hasta EndDraw.
pub fn pintar(g: &Graficos, ordenes: &[Orden]) -> Option<Vec<(u32, Com)>> {
    let c = &g.contexto;
    let letras = g.letras.borrow();
    let mut pinceles: Vec<(u32, Com)> = Vec::new();
    for o in ordenes {
        let (Orden::Caja(_, color) | Orden::Letras { color, .. }) = o;
        let k = match pinceles.iter().position(|p| p.0 == *color) {
            Some(k) => k,
            None => {
                let rgb = [(color & 0xFF) as f32 / 255.0, (color >> 8 & 0xFF) as f32 / 255.0, (color >> 16 & 0xFF) as f32 / 255.0, 1.0];
                let mut p = std::ptr::null_mut();
                pinceles.push((*color, Com::de(unsafe { f!(c, 8, fn(*const [f32; 4], *const c_void, *mut *mut c_void) -> i32)(c.0, &rgb, std::ptr::null(), &mut p) }, p)?));
                pinceles.len() - 1
            }
        };
        let pincel = pinceles[k].1 .0;
        match o {
            Orden::Caja(r, _) => unsafe { f!(c, 17, fn(*const [f32; 4], *mut c_void) -> ())(c.0, r, pincel) },
            Orden::Letras { x, y, letra, em, glifos, avances, texto, grupos, .. } => {
                let corrida = Corrida {
                    cara: letras[*letra].cara.0,
                    em: *em,
                    cuantos: glifos.len() as u32,
                    glifos: glifos.as_ptr(),
                    avances: avances.as_ptr(),
                    desplazamientos: std::ptr::null(),
                    de_lado: 0,
                    nivel: 0,
                };
                let d = Descripcion { idioma: std::ptr::null(), texto: texto.as_ptr(), largo: texto.len() as u32, grupos: grupos.as_ptr(), posicion: 0 };
                let d: *const Descripcion = if texto.is_empty() { std::ptr::null() } else { &d };
                unsafe { f!(c, 82, fn(Punto, *const Corrida, *const Descripcion, *mut c_void, u32) -> ())(c.0, Punto(*x, *y), &corrida, d, pincel, 0) };
            }
        }
    }
    Some(pinceles)
}
