// Markdown en el editor: cómo se ve cada línea (en RTF, para el control de texto) y las ayudas de la barra de
// formato. Lo que se ve nunca cambia el texto: las marcas (**, ##, `...) siguen ahí, solo que ocultas (texto oculto
// del control, que no ocupa nada), o tenues con «Mostrar marcas de formato».

const CR: u16 = 13;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Valla {
    ch: u16,
    largo: usize,
}

pub struct Colores {
    pub texto: u32,
    pub apagado: u32,
    pub acento: u32,
    pub fondo_codigo: u32,
    pub cita: u32,
    pub fondo: u32,
    /// Lo que dibuja Bloc: la rejilla de las tablas, la raya `---` y la de debajo de los títulos, y el fondo y la barra
    /// de las citas.
    pub rejilla: u32,
    pub raya: u32,
    pub fondo_cita: u32,
    pub barra_cita: u32,
}

// Índices de la tabla de colores del RTF.
const C_TEXTO: u8 = 1;
const C_APAGADO: u8 = 2;
const C_ACENTO: u8 = 3;
const C_FONDO_CODIGO: u8 = 4;
const C_CITA: u8 = 5;
const C_FONDO: u8 = 6;

// Tamaños en medios puntos: el texto va a 11 pt y los títulos, como en la vista previa del Explorador (2, 1,5, 1,25 y 1
// veces el texto; 0,875 y 0,85 los dos últimos).
pub const TAM: u16 = 22;
pub const TAM_CODIGO: u16 = 20;
// Las citas, a 14 px de los 16 de la vista previa.
const TAM_CITA: u16 = 19;
const TAM_TITULO: [u16; 6] = [44, 33, 28, 22, 19, 19];
// Twips: unos 400 píxeles.
pub const ANCHO_RAYA: usize = 6000;
// Lo más que puede ocupar una tabla alineada, en caracteres.
const MAX_ALINEADO: usize = 2_000_000;

// Lo que se sabe de cada carácter de una línea.
pub const NEGRITA: u8 = 1;
pub const CURSIVA: u8 = 2;
pub const TACHADO: u8 = 4;
pub const CODIGO: u8 = 8;
pub const ENLACE: u8 = 16;
const MARCA: u8 = 32;
const PROTEGIDO: u8 = 64; // ya no puede ser marca de otra cosa (dentro de código, de una dirección...)
const ACENTO: u8 = 128; // marca de lista: se ve siempre, en color

// Más allá, una línea se enseña sin formato de dentro: un texto raro nunca hace esperar al teclear.
const MAX_LINEA: usize = 20_000;
// Lo más lejos que se busca el cierre de una marca, como hace la vista de GitHub.
const MAX_TRAMO: usize = 3000;

#[derive(Clone, Copy, PartialEq)]
enum Tipo {
    Parrafo,
    Titulo(usize),
    /// Con el nivel del título que lleva dentro (0, ninguno).
    Cita(usize),
    Tabla,
    Codigo,
    Valla,
    Regla,
}

/// Una pareja de marcas en una línea: la de abrir va de `abre` a `abre + n`; la de cerrar, de `cierra` a `fin`. En un
/// enlace, lo de dentro es el texto que se ve.
#[derive(Clone, Copy, PartialEq, Debug)]
struct Par {
    abre: usize,
    n: usize,
    cierra: usize,
    fin: usize,
    efecto: u8,
}

/// El formato de letra de un tramo: Consolas o Segoe UI, el tamaño en medios puntos y los colores por su índice (el
/// fondo, 0 si no lleva).
#[derive(Clone, Copy, PartialEq)]
pub struct Estilo {
    pub mono: bool,
    pub tam: u16,
    pub color: u8,
    pub fondo: u8,
    pub negrita: bool,
    pub cursiva: bool,
    pub tachado: bool,
    pub subrayado: bool,
    /// Twips de más entre letra y letra.
    pub espacio: u16,
    /// Texto oculto del control: no ocupa nada (ver `linea`).
    pub oculto: bool,
}

/// El color de índice `i` (de 1 a 6, como en la tabla de colores del RTF).
pub fn color(c: &Colores, i: u8) -> u32 {
    [c.texto, c.apagado, c.acento, c.fondo_codigo, c.cita, c.fondo][i as usize - 1]
}

/// Con `vista` (las marcas ocultas), el interlineado de la vista previa del Explorador: el 1,5 de su hoja va sobre la
/// letra, y el de RichEdit sobre el alto de la línea de Segoe UI (1,33 veces la letra): 1,15.
pub fn cabecera(c: &Colores, vista: bool) -> Vec<u8> {
    // Con el juego de caracteres en la tabla de letras, todas quedan con el mismo, como al ponerlas por tramos (sin él,
    // RichEdit cambia de juego en la primera ñ o tilde de cada tramo y parte ahí lo que dibuja).
    let mut r = b"{\\rtf1\\ansi\\deff0\\uc1{\\fonttbl{\\f0\\fswiss\\fcharset0 Segoe UI;}{\\f1\\fmodern\\fcharset0 Consolas;}{\\f2\\fswiss\\fcharset0 Segoe UI Semibold;}}{\\colortbl;".to_vec();
    for col in (1..=6).map(|i| color(c, i)) {
        r.extend(format!("\\red{}\\green{}\\blue{};", col & 0xFF, (col >> 8) & 0xFF, (col >> 16) & 0xFF).bytes());
    }
    // \pard: los párrafos que entran quedan normales, y no con el formato del sitio en que caen (una viñeta, por
    // ejemplo). Al pintar, luego solo se pone el de los que no lo son.
    r.extend(format!("}}\\pard{}\\f0\\fs{TAM}\\cf{C_TEXTO} ", if vista { "\\sl276\\slmult1" } else { "" }).bytes());
    r
}

pub fn espacio(c: u16) -> bool {
    char::from_u32(c as u32).is_some_and(char::is_whitespace)
}

fn alfanumerico(c: u16) -> bool {
    char::from_u32(c as u32).is_some_and(char::is_alphanumeric)
}

fn sangria(s: &[u16]) -> usize {
    s.iter().take_while(|&&c| c == b' ' as u16).count()
}

/// La de los elementos de lista, que también admite tabuladores: con ellos anida Obsidian.
fn sangria_lista(s: &[u16]) -> usize {
    s.iter().take_while(|&&c| es(c, b' ') || es(c, b'\t')).count()
}

fn es(c: u16, b: u8) -> bool {
    c == b as u16
}

/// Si la línea abre o cierra un bloque de código (``` o ~~~), su valla.
fn valla(s: &[u16]) -> Option<Valla> {
    let n = sangria(s);
    if n >= 4 {
        return None;
    }
    let ch = *s.get(n)?;
    if !es(ch, b'`') && !es(ch, b'~') {
        return None;
    }
    let largo = s[n..].iter().take_while(|&&c| c == ch).count();
    // Una valla de comillas no lleva comillas detrás: ```a``b``` es código en línea.
    (largo >= 3 && !(es(ch, b'`') && s[n + largo..].contains(&ch))).then_some(Valla { ch, largo })
}

/// Como en CommonMark: hasta 3 espacios delante, una tirada seguida del mismo carácter (no menos que al abrir) y luego
/// solo espacios.
fn cierra(s: &[u16], v: Valla) -> bool {
    let n = sangria(s);
    let largo = s[n..].iter().take_while(|&&c| c == v.ch).count();
    n < 4 && largo >= v.largo && s[n + largo..].iter().all(|&c| espacio(c))
}

/// Cómo queda la valla abierta al final de la línea s, si se empezó con `antes`.
fn tras_linea(s: &[u16], antes: Option<Valla>) -> Option<Valla> {
    match antes {
        Some(v) => if cierra(s, v) { None } else { Some(v) },
        None => valla(s),
    }
}

/// El bloque de código abierto al empezar la línea que empieza en `ini` (o None).
pub fn estado_hasta(texto: &[u16], ini: usize) -> Option<Valla> {
    if ini == 0 { None } else { estado_tras(&texto[..ini - 1], None) }
}

/// ¿La línea abre un bloque de código o es una raya?
pub fn valla_o_regla(s: &[u16]) -> bool {
    valla(s).is_some() || regla(s)
}

/// ¿Hay alguna valla en estas líneas? Si no hay ninguna en todo el texto, no hace falta mirar desde el principio.
pub fn hay_valla(texto: &[u16]) -> bool {
    texto.split(|&c| c == CR).any(|l| valla(l).is_some())
}

/// Cómo queda el bloque de código al final de estas líneas, empezando con `antes`.
pub fn estado_tras(texto: &[u16], antes: Option<Valla>) -> Option<Valla> {
    texto.split(|&c| c == CR).fold(antes, |e, l| tras_linea(l, e))
}

fn titulo(s: &[u16]) -> Option<(usize, usize, usize)> {
    // ^ {0,3}(#{1,6})(?:[ \t]+(.*?))?(?:[ \t]+#+)?[ \t]*$ → (nivel, fin de la marca de delante, inicio de la de detrás)
    let n = sangria(s);
    if n > 3 {
        return None;
    }
    let h = s[n..].iter().take_while(|&&c| es(c, b'#')).count();
    if h == 0 || h > 6 {
        return None;
    }
    let mut i = n + h;
    if i < s.len() && !es(s[i], b' ') && !es(s[i], b'\t') {
        return None;
    }
    while i < s.len() && (es(s[i], b' ') || es(s[i], b'\t')) {
        i += 1;
    }
    // Marca de cierre: espacios, almohadillas y espacios al final.
    let mut f = s.len();
    while f > i && (es(s[f - 1], b' ') || es(s[f - 1], b'\t')) {
        f -= 1;
    }
    let mut g = f;
    while g > i && es(s[g - 1], b'#') {
        g -= 1;
    }
    let detras = if g < f && (g == i || es(s[g - 1], b' ') || es(s[g - 1], b'\t')) {
        let mut k = g;
        while k > i && (es(s[k - 1], b' ') || es(s[k - 1], b'\t')) {
            k -= 1;
        }
        k
    } else {
        f
    };
    Some((h, i, detras.max(i)))
}

fn regla(s: &[u16]) -> bool {
    // ^ {0,3}([-*_])([ \t]*\1){2,}[ \t]*$
    let n = sangria(s);
    if n > 3 || n >= s.len() {
        return false;
    }
    let c = s[n];
    if !es(c, b'-') && !es(c, b'*') && !es(c, b'_') {
        return false;
    }
    let mut cuantos = 0;
    for &x in &s[n..] {
        if x == c {
            cuantos += 1;
        } else if !es(x, b' ') && !es(x, b'\t') {
            return false;
        }
    }
    cuantos >= 3
}

/// Marca de lista: ^( *)([-*+]|\d{1,9}[.)])( +|$) → fin de la marca (con los espacios) y fin de la casilla si la hay.
fn item(s: &[u16]) -> Option<(usize, usize)> {
    let n = sangria_lista(s);
    let mut i = n;
    if i < s.len() && (es(s[i], b'-') || es(s[i], b'*') || es(s[i], b'+')) {
        i += 1;
    } else {
        let d = s[i..].iter().take_while(|&&c| (b'0' as u16..=b'9' as u16).contains(&c)).count();
        if d == 0 || d > 9 || i + d >= s.len() || !(es(s[i + d], b'.') || es(s[i + d], b')')) {
            return None;
        }
        i += d + 1;
    }
    if i < s.len() && !es(s[i], b' ') {
        return None;
    }
    while i < s.len() && es(s[i], b' ') {
        i += 1;
    }
    let mut c = i;
    if s.len() >= i + 3 && es(s[i], b'[') && (es(s[i + 1], b' ') || es(s[i + 1], b'x') || es(s[i + 1], b'X')) && es(s[i + 2], b']')
        && (s.len() == i + 3 || es(s[i + 3], b' '))
    {
        c = (i + 4).min(s.len());
    }
    Some((i, c))
}

fn puntuacion(c: u16) -> bool {
    matches!(c, 0x21..=0x2F | 0x3A..=0x40 | 0x5B..=0x60 | 0x7B..=0x7E)
}

fn empieza(s: &[u16], i: usize, p: &str) -> bool {
    let p = p.as_bytes();
    s.len() >= i + p.len() && s[i..i + p.len()].iter().zip(p).all(|(&a, b)| u8::try_from(a).is_ok_and(|a| a.eq_ignore_ascii_case(b)))
}

/// El formato de dentro de un trozo de línea: escapes, código, enlaces, negrita, cursiva, tachado. Las parejas de
/// marcas que encuentra van a `p`.
fn en_linea(s: &[u16], f: &mut [u8], p: &mut Vec<Par>, ini: usize, fin: usize) {
    let libre = |f: &[u8], i: usize| f[i] & (MARCA | PROTEGIDO) == 0;

    // Escapes: \* se ve como *. Van antes que el código: \` no lo abre.
    for i in ini..fin.saturating_sub(1) {
        if es(s[i], b'\\') && libre(f, i) && libre(f, i + 1) && puntuacion(s[i + 1]) {
            f[i] |= MARCA | PROTEGIDO;
            f[i + 1] |= PROTEGIDO;
        }
    }

    // Código: `...` con el mismo número de comillas al cerrar. Dentro no hay escapes: \ es una barra más.
    let mut i = ini;
    while i < fin {
        if !es(s[i], b'`') || !libre(f, i) {
            i += 1;
            continue;
        }
        let n = s[i..fin].iter().take_while(|&&c| es(c, b'`')).count();
        let mut j = i + n;
        let mut cierre = None;
        while j < fin && j < i + n + MAX_TRAMO {
            if es(s[j], b'`') {
                let m = s[j..fin].iter().take_while(|&&c| es(c, b'`')).count();
                if m == n {
                    cierre = Some(j);
                    break;
                }
                j += m;
            } else {
                j += 1;
            }
        }
        match cierre {
            Some(j) if j > i + n => {
                // Un espacio a cada lado (con algo más que espacios dentro) es relleno, como en CommonMark: va con las
                // comillas, se esconde con ellas y se quita con ellas.
                let dentro = &s[i + n..j];
                let pad = (dentro.len() >= 2 && es(dentro[0], b' ') && es(dentro[dentro.len() - 1], b' ') && !dentro.iter().all(|&c| es(c, b' '))) as usize;
                f[i..i + n + pad].fill(MARCA | PROTEGIDO);
                f[i + n + pad..j - pad].iter_mut().for_each(|x| *x = (*x & !MARCA) | CODIGO | PROTEGIDO);
                f[j - pad..j + n].fill(MARCA | PROTEGIDO);
                p.push(Par { abre: i, n: n + pad, cierra: j - pad, fin: j + n, efecto: CODIGO });
                i = j + n;
            }
            _ => i += n,
        }
    }

    // Enlaces e imágenes: [texto](dirección).
    let mut i = ini;
    while i < fin {
        if !es(s[i], b'[') || !libre(f, i) {
            i += 1;
            continue;
        }
        let mut j = i + 1;
        let mut nivel = 0;
        let mut cierre = None;
        while j < fin && j < i + MAX_TRAMO {
            if libre(f, j) && es(s[j], b'[') {
                nivel += 1;
                if nivel > 1 {
                    break;
                }
            } else if libre(f, j) && es(s[j], b']') {
                if nivel == 0 {
                    cierre = Some(j);
                    break;
                }
                nivel -= 1;
            }
            j += 1;
        }
        let imagen = i > ini && es(s[i - 1], b'!') && libre(f, i - 1);
        let Some(j) = cierre.filter(|&j| (j > i + 1 || imagen) && j + 1 < fin && es(s[j + 1], b'(')) else {
            i += 1;
            continue;
        };
        let mut k = j + 2;
        let mut parentesis = 0;
        let mut fin_dir = None;
        while k < fin && k < j + MAX_TRAMO {
            // Un paréntesis escapado (\) o \() es parte de la dirección.
            if !libre(f, k) {
            } else if es(s[k], b'(') {
                parentesis += 1;
            } else if es(s[k], b')') {
                if parentesis == 0 {
                    fin_dir = Some(k);
                    break;
                }
                parentesis -= 1;
            }
            k += 1;
        }
        let Some(k) = fin_dir else {
            i += 1;
            continue;
        };
        if imagen {
            f[i - 1] |= MARCA | PROTEGIDO;
        }
        f[i] |= MARCA | PROTEGIDO;
        f[i + 1..j].iter_mut().for_each(|x| *x |= ENLACE);
        f[j..=k].iter_mut().for_each(|x| *x |= MARCA | PROTEGIDO);
        let abre = i - imagen as usize;
        p.push(Par { abre, n: i + 1 - abre, cierra: j, fin: k + 1, efecto: ENLACE });
        i = k + 1;
    }

    // Direcciones: <https://...> y las que van sueltas.
    let mut i = ini;
    // Hasta aquí ningún < cierra con >: el que se busque desde antes de `corte` acabaría en el mismo sitio. Sin esto,
    // una línea con muchos «<http://» sin cerrar costaba el cuadrado de su largo.
    let mut corte = ini;
    while i < fin {
        if !libre(f, i) {
            i += 1;
            continue;
        }
        if i >= corte
            && es(s[i], b'<')
            && (empieza(&s[..fin], i + 1, "http://") || empieza(&s[..fin], i + 1, "https://") || empieza(&s[..fin], i + 1, "mailto:"))
        {
            let d = s[i + 1..fin].iter().position(|&c| es(c, b'>') || espacio(c));
            corte = d.map_or(fin, |d| i + 1 + d);
            if let Some(d) = d {
                let k = i + 1 + d;
                if es(s[k], b'>') {
                    f[i] |= MARCA | PROTEGIDO;
                    f[i + 1..k].iter_mut().for_each(|x| *x |= ENLACE | PROTEGIDO);
                    f[k] |= MARCA | PROTEGIDO;
                    p.push(Par { abre: i, n: 1, cierra: k, fin: k + 1, efecto: ENLACE });
                    i = k + 1;
                    continue;
                }
            }
        }
        let suelta = (empieza(&s[..fin], i, "http://") || empieza(&s[..fin], i, "https://") || empieza(&s[..fin], i, "www."))
            && (i == ini || !(alfanumerico(s[i - 1]) || es(s[i - 1], b'/')));
        if suelta {
            let mut k = i;
            while k < fin && !espacio(s[k]) && !es(s[k], b'<') && libre(f, k) {
                k += 1;
            }
            // La puntuación del final no es de la dirección; tampoco *, _ ni ~, que cierran un formato (como GitHub).
            while k > i && matches!(s[k - 1], 0x2E | 0x2C | 0x3B | 0x3A | 0x21 | 0x3F | 0x29 | 0x5D | 0x27 | 0x22 | 0x2A | 0x5F | 0x7E) {
                k -= 1;
            }
            f[i..k].iter_mut().for_each(|x| *x |= ENLACE | PROTEGIDO);
            p.push(Par { abre: i, n: 0, cierra: k, fin: k, efecto: ENLACE });
            i = k.max(i + 1);
            continue;
        }
        i += 1;
    }

    pares(s, f, p, ini, fin, b'*', 2, NEGRITA);
    pares(s, f, p, ini, fin, b'_', 2, NEGRITA);
    pares(s, f, p, ini, fin, b'*', 1, CURSIVA);
    pares(s, f, p, ini, fin, b'_', 1, CURSIVA);
    pares(s, f, p, ini, fin, b'~', 2, TACHADO);
}

/// Busca parejas de una marca (**, *, __, _, ~~) y da su efecto a lo de dentro. Como en GitHub: la marca de abrir
/// va pegada al texto por la derecha y la de cerrar por la izquierda; _ y __ no valen dentro de una palabra.
#[allow(clippy::too_many_arguments)]
fn pares(s: &[u16], f: &mut [u8], p: &mut Vec<Par>, ini: usize, fin: usize, ch: u8, n: usize, efecto: u8) {
    let libre = |f: &[u8], i: usize| f[i] & (MARCA | PROTEGIDO) == 0;
    let marca = |f: &[u8], i: usize| i + n <= fin && (i..i + n).all(|k| es(s[k], ch) && libre(f, k));
    let solo = |f: &[u8], i: usize| n == 2 || !(i + 1 < fin && es(s[i + 1], ch) && libre(f, i + 1));
    let raya = ch == b'_';
    // Los cierres posibles no dependen de dónde se abra: se apuntan una vez y se recorren a la vez que las aperturas.
    let cierres: Vec<usize> = (ini + 1..fin)
        .filter(|&j| {
            marca(f, j) && !espacio(s[j - 1]) && (n == 2 || !(es(s[j - 1], ch) && libre(f, j - 1)))
                && !(raya && j + n < fin && alfanumerico(s[j + n]))
        })
        .collect();
    let mut c = 0;
    let mut i = ini;
    while i + n < fin {
        let abre = marca(f, i) && solo(f, i) && !espacio(s[i + n]) && !(raya && i > ini && alfanumerico(s[i - 1]));
        if !abre {
            i += 1;
            continue;
        }
        while c < cierres.len() && cierres[c] < i + n + 1 {
            c += 1;
        }
        match cierres.get(c).copied().filter(|&j| j - i <= MAX_TRAMO && marca(f, j)) {
            Some(j) => {
                f[i..i + n].iter_mut().for_each(|x| *x |= MARCA | PROTEGIDO);
                f[i + n..j].iter_mut().for_each(|x| *x |= efecto);
                f[j..j + n].iter_mut().for_each(|x| *x |= MARCA | PROTEGIDO);
                p.push(Par { abre: i, n, cierra: j, fin: j + n, efecto });
                i = j + n;
            }
            None => i += 1,
        }
    }
}

/// Qué es la línea y el formato de cada carácter (en `f`), con las parejas de marcas de dentro (en `p`). Actualiza el
/// bloque de código abierto. Devuelve también dónde empieza y acaba lo que no es marca de la línea entera (la de un
/// título, una lista o una cita).
fn analizar(s: &[u16], abierto: &mut Option<Valla>, f: &mut Vec<u8>, p: &mut Vec<Par>) -> (Tipo, usize, usize) {
    f.clear();
    f.resize(s.len(), 0);
    p.clear();
    let corta = s.len() <= MAX_LINEA;
    match *abierto {
        Some(v) => {
            if cierra(s, v) {
                *abierto = None;
                (Tipo::Valla, 0, 0)
            } else {
                (Tipo::Codigo, 0, 0)
            }
        }
        None => {
            if let Some(v) = valla(s) {
                *abierto = Some(v);
                (Tipo::Valla, 0, 0)
            } else if let Some((nivel, a, b)) = titulo(s) {
                f[..a].fill(MARCA);
                f[b..].fill(MARCA);
                if corta {
                    en_linea(s, f, p, a, b);
                }
                (Tipo::Titulo(nivel), a, b)
            } else if regla(s) {
                (Tipo::Regla, 0, 0)
            } else if fila(s) {
                if corta {
                    en_linea(s, f, p, 0, s.len());
                }
                for (i, &c) in s.iter().enumerate() {
                    if es(c, b'|') && f[i] & PROTEGIDO == 0 {
                        f[i] |= MARCA;
                    }
                }
                (Tipo::Tabla, 0, s.len())
            } else if sangria(s) < 4 && s.get(sangria(s)).is_some_and(|&c| es(c, b'>')) {
                let i = marcas_cita(s).1;
                f[..i].iter_mut().for_each(|x| *x |= MARCA | ACENTO);
                // Dentro, un título o un elemento de lista (con su sangría, que lo anida), que se ven como tales.
                let (nivel, a, b) = match (titulo(&s[i..]), item(&s[i..])) {
                    (Some((n, a, b)), _) => {
                        f[i..i + a].fill(MARCA);
                        f[i + b..].fill(MARCA);
                        (n, i + a, i + b)
                    }
                    (None, Some((m, c))) => {
                        f[i..i + c].iter_mut().for_each(|x| *x |= MARCA | ACENTO);
                        (0, i + m.max(c), s.len())
                    }
                    _ => {
                        let a = i + sangria(&s[i..]);
                        f[i..a].iter_mut().for_each(|x| *x |= MARCA | ACENTO);
                        (0, a, s.len())
                    }
                };
                if corta {
                    en_linea(s, f, p, a, b);
                }
                (Tipo::Cita(nivel), a, b)
            } else if let Some((m, c)) = item(s) {
                f[..c].iter_mut().for_each(|x| *x |= MARCA | ACENTO);
                if corta {
                    en_linea(s, f, p, m.max(c), s.len());
                }
                (Tipo::Parrafo, m.max(c), s.len())
            } else {
                if corta {
                    en_linea(s, f, p, 0, s.len());
                }
                (Tipo::Parrafo, 0, s.len())
            }
        }
    }
}

/// Cómo es una línea como párrafo, para el propio control: la viñeta, la casilla o el número que dibuja él (no son
/// texto: el archivo no cambia), la sangría y si se pliega o se centra. Las medidas, en twips.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct Parrafo {
    /// 0, nada; 2, un número (`inicio`, con `estilo`); otro, ese carácter (•, ◦, ▪, ☐, ☑).
    pub numero: u16,
    pub inicio: u16,
    pub estilo: u16,
    pub sangria: i32,
    /// Cuánto más metidas van las líneas de después de la primera: ahí va lo que dibuja el control.
    pub colgado: i32,
    /// Sus letras no se ven: la fila |---| de una tabla, o cualquier fila de una tabla que dibuja Bloc encima.
    pub plegado: bool,
    /// Interlineado exacto, en twips (0, el normal): el alto de la fila de una tabla dibujada o el de una valla, que
    /// con las marcas ocultas es el relleno del bloque de código.
    pub alto: i32,
    /// La raya `---`: centrada y subrayada.
    pub raya: bool,
    /// Espacio antes y después, en twips (después de un título 1 o 2, el sitio de su raya), e interlineado sencillo (los
    /// títulos, como en la vista previa del Explorador).
    pub antes: i32,
    pub despues: i32,
    pub sencillo: bool,
    /// Sangría por la derecha, en twips: el relleno del código y el margen de las citas.
    pub derecha: i32,
    /// Una línea de un bloque de código: lleva el interlineado de su letra.
    pub codigo: bool,
}

// Cada nivel de lista, como los 40 px de la vista previa (sobre su letra de 16 px; aquí, sobre la de 11 pt).
const SANGRIA: i32 = 550;
// De la viñeta al texto, en twips: con su punto y el «- » escondido, el hueco de 13 px de la vista previa.
const VINETA: i32 = 225;
// La letra del salto de una línea en blanco con las marcas ocultas: con el interlineado de 1,15, su alto es el hueco
// entre párrafos de la vista previa (1em).
const TAM_HUECO: u16 = 14;
/// 1em (16 px en la vista previa) sobre la letra del texto, en twips.
pub const EM: i32 = TAM as i32 * 10;
/// El relleno de un bloque de código, en twips: arriba y abajo lo dan sus vallas, con las marcas ocultas.
pub const RELLENO: i32 = EM;
/// Cada nivel de cita, en twips: 40 px de margen, la barra (la dibuja Bloc, ver ventana::dibujar_rayas) y 1em hasta el
/// texto; por la derecha, el margen y 1em.
pub const MARGEN_CITA: i32 = SANGRIA;
pub const BARRA_CITA: i32 = EM / 4;
pub const CITA: i32 = MARGEN_CITA + BARRA_CITA + EM;
const CITA_DERECHA: i32 = MARGEN_CITA + EM;
const NUMERADA: u16 = 2; // PFN_ARABIC
const NUEVO_NUMERO: u16 = 0x8000; // PFNS_NEWNUMBER: cada elemento con su número, el que pone el texto

/// Las marcas de cita del principio de una línea: cuántas hay y dónde acaban. Cada una lleva como mucho un espacio
/// detrás; los demás son la sangría de lo que va dentro (una lista anidada).
fn marcas_cita(s: &[u16]) -> (usize, usize) {
    let (mut n, mut i) = (0, sangria(s));
    while s.get(i).is_some_and(|&c| es(c, b'>')) {
        n += 1;
        i += 1;
        if s.get(i).is_some_and(|&c| es(c, b' ')) {
            i += 1;
        }
        let k = i + sangria(&s[i..]);
        if s.get(k).is_some_and(|&c| es(c, b'>')) {
            i = k;
        }
    }
    (n, i)
}

/// ¿Es s una línea de un párrafo, con algo escrito (y fuera de un bloque de código)? Las que se pueden juntar (ver
/// `salto_blando`).
pub fn de_parrafo(s: &[u16]) -> bool {
    tipo_de_bloque(s) == Tipo::Parrafo && !s.iter().all(|&c| espacio(c))
}

/// Lo que dibuja Bloc encima de la línea s, fuera de un bloque de código (ver ventana::dibujar_rayas), sin analizar su
/// letra: si es una raya `---`, si es un título 1 o 2 (raya debajo) y en cuántas citas va, una dentro de otra.
pub fn de_encima(s: &[u16]) -> (bool, bool, usize) {
    match tipo_de_bloque(s) {
        Tipo::Regla => (true, false, 0),
        Tipo::Titulo(_) => (false, titulo(s).is_some_and(|t| t.0 <= 2), 0),
        Tipo::Cita(_) => (false, false, marcas_cita(s).0),
        _ => (false, false, 0),
    }
}

/// El párrafo de una línea ya analizada, con las marcas ocultas (con ellas a la vista, solo la raya va distinta).
fn parrafo(s: &[u16], tipo: Tipo, ocultar: bool) -> Parrafo {
    let mut p = Parrafo::default();
    match tipo {
        Tipo::Regla => p.raya = true,
        Tipo::Tabla if ocultar && separador(s) => p.plegado = true,
        Tipo::Cita(_) if ocultar => {
            let (niveles, marcas) = marcas_cita(s);
            let niveles = niveles as i32;
            // Con una lista dentro, el control dibuja su viñeta o su número dentro de la cita.
            let mut l = parrafo(&s[marcas..], Tipo::Parrafo, true);
            l.sangria += niveles.min(8) * CITA;
            l.derecha = niveles.min(8) * CITA_DERECHA;
            // Una línea con solo las marcas separa dos párrafos de la cita: RichEdit le da el alto de sus letras, que no
            // se ven, y no el del salto.
            if s[marcas..].iter().all(|&c| espacio(c)) {
                l.alto = EM;
            }
            return l;
        }
        Tipo::Valla if ocultar => p.alto = RELLENO,
        Tipo::Codigo if ocultar => (p.sangria, p.derecha, p.codigo) = (RELLENO, RELLENO, true),
        Tipo::Parrafo if ocultar => {
            let Some((m, c)) = item(s) else { return p };
            let n = sangria_lista(s);
            let tabs = s[..n].iter().filter(|&&c| es(c, b'\t')).count();
            let nivel = (tabs + (n - tabs) / 2).min(8) as i32;
            // La viñeta a media sangría y el texto a la sangría entera de su nivel, como en la vista previa.
            p.sangria = nivel * SANGRIA + SANGRIA / 2;
            p.colgado = SANGRIA / 2;
            let cifras = s[n..].iter().take_while(|&&c| (b'0' as u16..=b'9' as u16).contains(&c)).count();
            if c > m {
                p.numero = if es(s[m + 1], b' ') { 0x2610 } else { 0x2611 };
            } else if cifras == 0 {
                p.numero = [0x2022, 0x25E6, 0x25AA][nivel as usize % 3];
                (p.sangria, p.colgado) = (p.sangria + p.colgado - VINETA, VINETA);
            } else if let Ok(k) = String::from_utf16_lossy(&s[n..n + cifras]).parse::<u16>() {
                p.numero = NUMERADA;
                p.inicio = k;
                p.estilo = NUEVO_NUMERO | if es(s[n + cifras], b')') { 0 } else { 0x200 };
                p.colgado = SANGRIA / 2 + 120 * (cifras as i32 - 1).max(0);
            }
        }
        // Los títulos, con su interlineado (el de la vista previa, 1,25, es casi el sencillo de Segoe UI); el 1 y el 2,
        // con sitio debajo para su raya: 0,3 de su letra y un punto.
        Tipo::Titulo(n) if ocultar => {
            p.sencillo = true;
            if n <= 2 {
                p.despues = TAM_TITULO[n - 1] as i32 * 3 + 15;
            }
        }
        _ => {}
    }
    p
}

/// La letra del salto de detrás de la línea l, seguida de m (`en_codigo`: l va dentro de un bloque de código). Con las
/// marcas ocultas, el de una línea en blanco es el hueco entre párrafos de la vista previa, 1em, y 1,5em antes de un
/// título; el de una línea de código o de cita, de su letra: el salto, más alto, haría más alta su última línea.
pub fn salto(l: &[u16], m: Option<&[u16]>, en_codigo: bool, ocultar: bool) -> Estilo {
    if !ocultar {
        return base_texto();
    }
    if en_codigo {
        return Estilo { mono: true, tam: TAM_CODIGO, ..base_texto() };
    }
    let tam = if tipo_de_bloque(l) == Tipo::Cita(0) {
        TAM_CITA
    } else if !l.iter().all(|&c| espacio(c)) || m.is_some_and(|m| titulo(m).is_some()) {
        TAM
    } else {
        TAM_HUECO
    };
    Estilo { tam, ..base_texto() }
}

/// La lista que sigue abierta al acabar una línea (con las marcas ocultas): la columna en que empieza el texto de su
/// último elemento, dónde lo pone el control (twips) y si ha habido una línea en blanco desde entonces. Lo de debajo con
/// al menos esa sangría, o pegado sin blanca entre medias, sigue en ese elemento y va a la altura de su texto.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct EnLista {
    columna: usize,
    texto: i32,
    tras_blanca: bool,
}

/// Cómo queda la lista tras la línea s, de tipo `tipo`, si venía `antes`; y si s continúa un elemento.
fn tras_en_lista(s: &[u16], tipo: Tipo, antes: Option<EnLista>) -> (Option<EnLista>, bool) {
    if tipo != Tipo::Parrafo {
        return (None, false);
    }
    if s.iter().all(|&c| espacio(c)) {
        return (antes.map(|l| EnLista { tras_blanca: true, ..l }), false);
    }
    if let Some((m, _)) = item(s) {
        let p = parrafo(s, Tipo::Parrafo, true);
        return (Some(EnLista { columna: m, texto: p.sangria + p.colgado, tras_blanca: false }), false);
    }
    match antes {
        Some(l) if !l.tras_blanca || sangria_lista(s) >= l.columna => (Some(EnLista { tras_blanca: false, ..l }), true),
        _ => (None, false),
    }
}

/// El tipo de una línea fuera de un bloque de código, sin mirar su formato de letra: como lo decide `analizar`.
fn tipo_de_bloque(s: &[u16]) -> Tipo {
    let n = sangria(s);
    if valla(s).is_some() {
        Tipo::Valla
    } else if titulo(s).is_some() {
        Tipo::Titulo(1)
    } else if regla(s) {
        Tipo::Regla
    } else if fila(s) {
        Tipo::Tabla
    } else if n < 4 && s.get(n).is_some_and(|&c| es(c, b'>')) {
        Tipo::Cita(0)
    } else {
        Tipo::Parrafo
    }
}

/// Cómo queda la lista tras estas líneas, si venía `antes`.
pub fn en_lista_tras(texto: &[u16], antes: Option<EnLista>) -> Option<EnLista> {
    texto.split(|&c| c == CR).fold(antes, |e, l| tras_en_lista(l, tipo_de_bloque(l), e).0)
}

/// La lista abierta al empezar la línea que empieza en `ini`. Se sube hasta la primera línea que no depende de las de
/// encima (un elemento o lo que no es párrafo), sin cruzar una valla y como mucho 1.000 líneas: así cuesta poco en
/// cualquier sitio de un documento largo y da lo mismo que pintarlo desde el principio.
pub fn en_lista_hasta(texto: &[u16], ini: usize) -> Option<EnLista> {
    let mut desde = ini;
    for _ in 0..1000 {
        if desde == 0 {
            break;
        }
        let i = texto[..desde - 1].iter().rposition(|&c| c == CR).map_or(0, |k| k + 1);
        let l = &texto[i..desde - 1];
        if valla(l).is_some() {
            break;
        }
        desde = i;
        if item(l).is_some() || tipo_de_bloque(l) != Tipo::Parrafo {
            break;
        }
    }
    if desde < ini { en_lista_tras(&texto[desde..ini - 1], None) } else { None }
}

/// Hasta dónde llegan, desde `pos` (un final de línea), las líneas que se pintan distinto si la lista abierta es `a` o
/// `b`: hasta que las dos coinciden, que es en el siguiente elemento o en lo que no es párrafo. Como mucho 500 líneas.
pub fn fin_de_lista(texto: &[u16], pos: usize, mut a: Option<EnLista>, mut b: Option<EnLista>) -> usize {
    let mut fin = pos;
    for l in texto.get(pos + 1..).unwrap_or(&[]).split(|&c| c == CR).take(500) {
        if a == b {
            break;
        }
        let tipo = tipo_de_bloque(l);
        (a, b) = (tras_en_lista(l, tipo, a).0, tras_en_lista(l, tipo, b).0);
        fin += 1 + l.len();
    }
    fin.min(texto.len())
}

/// Pinta una línea y actualiza el bloque de código abierto. Con `ocultar`, las marcas no se ven en ninguna línea (ni en
/// la del cursor: enseñarlas al llegar él movía la línea entera); sin él, se ven tenues. `cabecera`: es la primera fila
/// de una tabla (la que va encima de |---|); `tras_fila`, la de encima es una fila (si no, un |---| no se pliega);
/// `cerrada`, el alto en twips si es una fila de una tabla que dibuja Bloc. `lista`: la lista abierta antes de la línea; queda la de después. Sus tramos de letra van a `tramos`, con la línea
/// empezando en `desde`. Devuelve cómo es como párrafo.
#[allow(clippy::too_many_arguments)]
fn linea(
    rtf: &mut Vec<u8>,
    tramos: &mut Vec<Tramo>,
    desde: usize,
    s: &[u16],
    abierto: &mut Option<Valla>,
    lista: &mut Option<EnLista>,
    ocultar: bool,
    imprimir: bool,
    cabecera: bool,
    tras_fila: bool,
    cerrada: Option<i32>,
    blando: bool,
    f: &mut Vec<u8>,
    p: &mut Vec<Par>,
) -> Parrafo {
    let (tipo, contenido, fin_contenido) = analizar(s, abierto, f, p);
    let mut par = parrafo(s, tipo, ocultar);
    // La raya `---` de la vista previa lleva 18 px por encima y por debajo: con las líneas en blanco de alrededor, la
    // suya casi no tiene alto. Al imprimir es su subrayado y necesita la línea entera.
    if ocultar && !imprimir && tipo == Tipo::Regla {
        par.alto = EM / 4;
    }
    par.plegado &= tras_fila;
    if let Some(alto) = cerrada {
        (par.plegado, par.alto) = (true, alto);
    }
    let continua;
    (*lista, continua) = if ocultar { tras_en_lista(s, tipo, *lista) } else { (None, false) };
    if let Some(l) = lista.filter(|_| continua) {
        par.sangria = l.texto;
    }
    // Tras un salto blando, la sangría de delante tampoco se ve: el hueco ya lo pone el salto.
    let sangria_oculta = if continua || blando { sangria_lista(s) } else { 0 };
    let base = match tipo {
        Tipo::Parrafo => base_texto(),
        Tipo::Titulo(n) => Estilo { tam: TAM_TITULO[n - 1], negrita: true, ..base_texto() },
        Tipo::Cita(0) => Estilo { tam: TAM_CITA, color: C_CITA, ..base_texto() },
        Tipo::Cita(n) => Estilo { tam: TAM_TITULO[n - 1], negrita: true, color: C_CITA, ..base_texto() },
        Tipo::Tabla => Estilo { mono: true, tam: TAM_CODIGO, negrita: cabecera, ..base_texto() },
        Tipo::Codigo => Estilo { mono: true, tam: TAM_CODIGO, fondo: C_FONDO_CODIGO, ..base_texto() },
        Tipo::Valla => Estilo { mono: true, tam: TAM_CODIGO, color: C_APAGADO, ..base_texto() },
        // Guiones del color del fondo. En pantalla la línea la dibuja Bloc encima (ventana::dibujar_rayas): con espaciado
        // de letra, aunque sea en una sola raya, RichEdit tarda tres veces y media más en colocar todo el documento. Al
        // imprimir, separados para que midan lo mismo sean tres o veinte, y la línea es su subrayado (editor::raya).
        Tipo::Regla => Estilo { color: C_FONDO, espacio: if imprimir { (ANCHO_RAYA / s.len().max(1)) as u16 } else { 0 }, ..base_texto() },
    };
    // Las tablas se leen alineadas: sus marcas no encogen. Las vallas son todo marca. Las marcas de lista se ven, salvo
    // si dibuja la suya el control; las de cita, nunca: su barra la dibuja Bloc.
    let dibujada = par.numero != 0 || matches!(tipo, Tipo::Cita(_));
    let escondible = |i: usize| match tipo {
        _ if par.plegado || i < sangria_oculta => true,
        Tipo::Tabla | Tipo::Regla => false,
        Tipo::Valla => true,
        _ => f[i] & MARCA != 0 && (f[i] & ACENTO == 0 || dibujada),
    };
    // Las marcas escondidas del principio de la línea (lista, título, cita, sangría) y las vallas van en letras de un
    // punto del color del fondo, que miden un píxel cada una: Bloc pone el cursor y la selección entre ellas (los
    // botones de párrafo, Tab, Retroceso). Las de dentro del texto (negrita, código, enlaces) y lo escondido del
    // principio de una línea que sigue un párrafo juntado (el hueco ya lo pone el salto), en texto oculto del control,
    // que no ocupa nada; RichEdit saca de él la selección (ver editor::seleccionar).
    let mut oculto = vec![false; s.len()];
    if ocultar && !par.plegado {
        let delante = if blando { (0..s.len()).take_while(|&i| escondible(i)).count() } else { 0 };
        oculto[..delante].fill(true);
        if matches!(tipo, Tipo::Parrafo | Tipo::Titulo(_) | Tipo::Cita(_)) {
            (contenido..fin_contenido.min(s.len())).filter(|&i| escondible(i)).for_each(|i| oculto[i] = true);
        }
    }
    let diminuta = |i: usize| ocultar && escondible(i) && !oculto[i];
    let mut actual: Option<Estilo> = None;
    for (i, &c) in s.iter().enumerate() {
        let x = f[i];
        let e = if ocultar && escondible(i) {
            Estilo { mono: base.mono, tam: 2, color: C_FONDO, oculto: oculto[i], ..base_texto() }
        } else if x & ACENTO != 0 {
            Estilo { color: if matches!(tipo, Tipo::Cita(_)) { C_APAGADO } else { C_ACENTO }, ..base }
        } else if x & MARCA != 0 {
            Estilo { color: C_APAGADO, ..base }
        } else {
            let mut e = base;
            if x & CODIGO != 0 {
                e.mono = true;
                e.tam = if base.tam > TAM { base.tam * 9 / 10 } else { TAM_CODIGO };
                e.fondo = C_FONDO_CODIGO;
            }
            if x & ENLACE != 0 {
                e.color = C_ACENTO;
                e.subrayado = true;
            }
            e.negrita |= x & NEGRITA != 0;
            e.cursiva |= x & CURSIVA != 0;
            e.tachado |= x & TACHADO != 0;
            // Un espacio junto a marcas de un punto, más estrecho: con ellas mide lo que un espacio.
            let pegadas = (0..i).rev().take_while(|&j| diminuta(j)).count() + (i + 1..s.len()).take_while(|&j| diminuta(j)).count();
            if es(c, b' ') && !e.mono && pegadas > 0 {
                e.tam = mas_estrecho(e.tam, ESPACIO, pegadas);
            }
            e
        };
        if actual != Some(e) {
            if actual.is_some() {
                rtf.push(b'}');
            }
            abrir(rtf, &e);
            actual = Some(e);
        }
        escapar(rtf, c);
        anotar(tramos, desde + i, e);
    }
    if actual.is_some() {
        rtf.push(b'}');
    }
    par
}

/// Lo que avanza el espacio de Segoe UI (y de Segoe UI Semibold), en cuadratines.
const ESPACIO: f32 = 0.2744;

/// El tamaño (en medios puntos) con que un espacio que avanza `avance` cuadratines en una letra de `tam` mide `marcas`
/// píxeles menos, a 96 ppp: lo que miden las marcas de un punto que tiene al lado, un píxel cada una. A más ppp, la
/// letra crece y las marcas no: el hueco queda algo más corto que un espacio, nunca más largo.
fn mas_estrecho(tam: u16, avance: f32, marcas: usize) -> u16 {
    let px = |t: f32| t * avance * 2.0 / 3.0;
    let quiere = (px(tam as f32).round() - marcas as f32).max(1.0);
    ((quiere / px(1.0)).round() as u16).clamp(2, tam)
}

/// Un tramo de letra: de dónde a dónde (desde el principio de lo pintado) y cómo es.
pub type Tramo = (usize, usize, Estilo);

/// Un carácter más, en `i`: alarga el último tramo si sigue igual.
fn anotar(tramos: &mut Vec<Tramo>, i: usize, e: Estilo) {
    match tramos.last_mut() {
        Some(t) if t.1 == i && t.2 == e => t.1 = i + 1,
        _ => tramos.push((i, i + 1, e)),
    }
}

fn base_texto() -> Estilo {
    Estilo { mono: false, tam: TAM, color: C_TEXTO, fondo: 0, negrita: false, cursiva: false, tachado: false, subrayado: false, espacio: 0, oculto: false }
}

fn abrir(rtf: &mut Vec<u8>, e: &Estilo) {
    rtf.extend(format!("{{\\f{}\\fs{}\\cf{}", letra_rtf(e), e.tam, e.color).bytes());
    if e.fondo != 0 {
        rtf.extend(format!("\\highlight{}", e.fondo).bytes());
    }
    if e.espacio > 0 {
        rtf.extend(format!("\\expndtw{}", e.espacio).bytes());
    }
    if e.oculto {
        rtf.extend_from_slice(b"\\v");
    }
    for (si, cual) in [(e.negrita && e.mono, &b"\\b"[..]), (e.cursiva, b"\\i"), (e.tachado, b"\\strike"), (e.subrayado, b"\\ul")] {
        if si {
            rtf.extend_from_slice(cual);
        }
    }
    rtf.push(b' ');
}

/// La letra de la tabla del RTF: Segoe UI, Consolas o, para la negrita, Segoe UI Semibold (la de la vista previa).
fn letra_rtf(e: &Estilo) -> u8 {
    if e.mono { 1 } else if e.negrita { 2 } else { 0 }
}

/// El nombre de la letra de ese formato, como en el RTF.
pub fn nombre_letra(e: &Estilo) -> &'static str {
    ["Segoe UI", "Consolas", "Segoe UI Semibold"][letra_rtf(e) as usize]
}

fn escapar(rtf: &mut Vec<u8>, c: u16) {
    match c {
        0x5C | 0x7B | 0x7D => {
            rtf.push(b'\\');
            rtf.push(c as u8);
        }
        9 => rtf.extend_from_slice(b"\\tab "),
        0x20..=0x7E => rtf.push(c as u8),
        _ => rtf.extend(format!("\\u{}?", c as i16).bytes()),
    }
}

/// Lo que hay que meter en el control para pintar unas líneas: el RTF, o lo mismo en tramos de letra (seguidos, de
/// todo lo pintado, con los saltos de línea como el RTF) y, de las líneas que no son párrafos normales, dónde empiezan
/// (desde el principio de lo pintado) y cómo son.
pub struct Pintado {
    pub rtf: Vec<u8>,
    pub tramos: Vec<Tramo>,
    pub parrafos: Vec<(usize, Parrafo)>,
    /// Los saltos blandos (desde el principio de lo pintado), que van en el control como JUNTO.
    pub blandos: Vec<usize>,
}

/// Lo que lleva el control en lugar de un salto blando: un espacio de un cuarto de cuadratín, que mide como el espacio
/// de Segoe UI y deja partir la línea detrás. El texto no cambia de largo y las posiciones son las mismas.
pub const JUNTO: u16 = 0x2005;
// Las líneas que se juntan en un párrafo como mucho: cada tecla repinta el párrafo entero.
const MAX_JUNTAS: usize = 200;

/// Si el salto entre las líneas l y m (fuera de un bloque de código y con las marcas ocultas) es blando: m sigue el
/// párrafo o el elemento de lista de l, y se ven juntas en un párrafo, como en la vista previa del Explorador. `lista`,
/// la que sigue abierta tras l.
fn salto_blando(l: &[u16], m: &[u16], lista: Option<EnLista>) -> bool {
    let texto = |s: &[u16]| tipo_de_bloque(s) == Tipo::Parrafo && !s.iter().all(|&c| espacio(c));
    // Dos espacios o una \ al final parten la línea, y una línea de = debajo hace título al párrafo.
    if l.ends_with(&[32, 32]) || l.last() == Some(&(b'\\' as u16)) || m.iter().all(|&c| es(c, b'=') || espacio(c)) {
        return false;
    }
    // Dos líneas de texto de la misma cita, sin listas.
    if tipo_de_bloque(l) == Tipo::Cita(0) && tipo_de_bloque(m) == Tipo::Cita(0) {
        let ((nl, il), (nm, im)) = (marcas_cita(l), marcas_cita(m));
        return nl == nm && texto(&l[il..]) && texto(&m[im..]) && item(&l[il..]).is_none() && item(&m[im..]).is_none();
    }
    if !texto(l) || !texto(m) || item(m).is_some() {
        return false;
    }
    match lista {
        Some(_) => tras_en_lista(m, Tipo::Parrafo, lista).1,
        None => sangria(l) < 4 && sangria(m) < 4,
    }
}

/// Las líneas de texto[ini..fin] (ini es inicio de línea; fin, final de línea sin su salto), empezando con el bloque de
/// código `abierto`. Con `ocultar`, sin marcas a la vista. `cerradas`: las filas de las tablas que dibuja Bloc (dónde
/// empiezan en `texto`, de menor a mayor, y su alto en twips), que se pliegan a ese alto sin que se vean sus letras. Con
/// `juntar`, los saltos blandos van como JUNTO (ver `salto_blando`); el de justo antes de `ini` cuenta como duro.
#[allow(clippy::too_many_arguments)]
pub fn pintar(c: &Colores, texto: &[u16], ini: usize, fin: usize, mut abierto: Option<Valla>, ocultar: bool, imprimir: bool, cerradas: &[(usize, i32)], juntar: bool) -> Pintado {
    let mut rtf = cabecera(c, ocultar);
    let (mut tramos, mut parrafos, mut blandos) = (Vec::new(), Vec::new(), Vec::new());
    let (mut f, mut p) = (Vec::new(), Vec::new());
    let mut lista = if ocultar && abierto.is_none() { en_lista_hasta(texto, ini) } else { None };
    let mut lineas = texto[ini..fin].split(|&ch| ch == CR).peekable();
    let mut pos = 0;
    // Las líneas de al lado, aunque queden fuera de lo pintado: una fila encima de |---| es la cabecera de la tabla.
    let previa = (ini > 0).then(|| &texto[texto[..ini - 1].iter().rposition(|&ch| ch == CR).map_or(0, |k| k + 1)..ini - 1]);
    let mut tras_fila = previa.is_some_and(fila);
    // La línea de antes y si iba dentro de un bloque de código; y cuántas van juntas en el párrafo.
    let mut anterior: Option<(&[u16], bool)> = None;
    let mut juntas = 0;
    // Las marcas de un punto con que acaba la línea de antes.
    let mut cola = 0;
    while let Some(l) = lineas.next() {
        let blando = juntar && abierto.is_none() && juntas < MAX_JUNTAS && anterior.is_some_and(|(a, en_codigo)| !en_codigo && salto_blando(a, l, lista));
        if pos > 0 {
            let mut e = anterior.map_or(base_texto(), |(a, en_codigo)| salto(a, Some(l), en_codigo, ocultar));
            // El salto blando (un cuarto de cuadratín) tras unas marcas de un punto, más estrecho, como un espacio.
            if blando && cola > 0 {
                e.tam = mas_estrecho(e.tam, 0.25, cola);
            }
            if blando && e != base_texto() {
                rtf.extend(format!("{{\\f{}\\fs{}\\u8197?}}", letra_rtf(&e), e.tam).bytes());
                blandos.push(pos - 1);
                juntas += 1;
            } else if blando {
                rtf.extend_from_slice(b"\\u8197?");
                blandos.push(pos - 1);
                juntas += 1;
            } else if e != base_texto() {
                rtf.extend(format!("{{\\f{}\\fs{}\\par}}", letra_rtf(&e), e.tam).bytes());
                juntas = 0;
            } else {
                rtf.extend_from_slice(b"\\par ");
                juntas = 0;
            }
            anotar(&mut tramos, pos - 1, e);
        }
        let en_codigo = abierto.is_some();
        let siguiente = lineas.peek().copied().or_else(|| texto.get(fin + 1..).map(|r| r.split(|&ch| ch == CR).next().unwrap_or(&[])));
        let cabecera = fila(l) && siguiente.is_some_and(|x| x.contains(&(b'|' as u16)) && separador(x));
        let cerrada = cerradas.binary_search_by_key(&(ini + pos), |x| x.0).ok().map(|k| cerradas[k].1);
        let mut par = linea(&mut rtf, &mut tramos, pos, l, &mut abierto, &mut lista, ocultar, imprimir, cabecera, tras_fila, cerrada, blando, &mut f, &mut p);
        cola = tramos.last().filter(|t| t.1 == pos + l.len() && t.2.tam == 2 && !t.2.oculto).map_or(0, |t| t.1 - t.0);
        // Un título pegado a lo de encima lleva el hueco de la vista previa (24 px) que, si no, da su línea en blanco.
        if par.sencillo && anterior.map(|x| x.0).or(previa).is_some_and(|a| !a.iter().all(|&c| espacio(c))) {
            par.antes = 330;
        }
        // La que sigue a un salto blando es del párrafo de la de antes.
        if par != Parrafo::default() && !blando {
            parrafos.push((pos, par));
        }
        tras_fila = fila(l);
        anterior = Some((l, en_codigo));
        pos += l.len() + 1;
    }
    rtf.push(b'}');
    Pintado { rtf, tramos, parrafos, blandos }
}

// --- Barra de formato ---

fn u(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}

/// Una línea como la ve la vista con formato, para ponerle o quitarle negrita, cursiva, tachado o código.
pub struct Linea {
    f: Vec<u8>,
    p: Vec<Par>,
    tipo: Tipo,
    ini: usize,
    fin: usize,
    /// La fila |---| de una tabla.
    separador: bool,
}

impl Linea {
    pub fn leer(s: &[u16], abierto: Option<Valla>) -> Linea {
        let (mut f, mut p, mut a) = (Vec::new(), Vec::new(), abierto);
        let (tipo, ini, fin) = analizar(s, &mut a, &mut f, &mut p);
        Linea { f, p, tipo, ini, fin, separador: tipo == Tipo::Tabla && separador(s) }
    }

    /// Dentro de un bloque de código, en una raya o en la fila |---| de una tabla no hay formato de letra.
    pub fn con_letra(&self) -> bool {
        !matches!(self.tipo, Tipo::Codigo | Tipo::Valla | Tipo::Regla) && !self.separador
    }


    pub fn en_codigo(&self) -> bool {
        matches!(self.tipo, Tipo::Codigo | Tipo::Valla)
    }

    fn pareja_en(&self, i: usize, efecto: u8) -> Option<Par> {
        self.p.iter().copied().find(|q| q.efecto == efecto && q.abre + q.n <= i && i <= q.cierra)
    }

    /// ¿Tiene ese efecto todo lo que hay en s[i..j], sin contar marcas ni espacios? None si no hay nada que contar. Con
    /// el cursor (i == j), si está dentro de una pareja de ese efecto.
    pub fn tiene(&self, s: &[u16], i: usize, j: usize, efecto: u8) -> Option<bool> {
        if !self.con_letra() {
            return None;
        }
        if i == j {
            return Some(self.pareja_en(i, efecto).is_some());
        }
        // Los espacios y una \ suelta no cuentan: las marcas nunca los cogen en los bordes.
        let neutro = |k: usize| espacio(s[k]) || es(s[k], b'\\') && self.f[k] & PROTEGIDO == 0;
        let mut hay = None;
        for k in i.max(self.ini)..j.min(self.fin) {
            if self.f[k] & MARCA == 0 && !neutro(k) {
                if self.f[k] & efecto == 0 {
                    return Some(false);
                }
                hay = Some(true);
            }
        }
        hay
    }

    /// Pone o quita un efecto a s[i..j] (sin selección, quita la pareja en que está el cursor). Las parejas de ese
    /// efecto que tocan lo seleccionado se rehacen: quitar la negrita a una palabra la saca de la frase en negrita, y
    /// ponérsela junto a otra las une. Devuelve la línea nueva y dónde quedan i y j.
    pub fn alternar(&self, s: &[u16], i0: usize, j0: usize, efecto: u8, poner: bool) -> (Vec<u16>, usize, usize) {
        let (mut i, mut j) = (i0.clamp(self.ini, self.fin), j0.clamp(self.ini, self.fin));
        if i == j {
            match self.pareja_en(i, efecto) {
                Some(q) if !poner => (i, j) = (q.abre + q.n, q.cierra),
                _ => return (s.to_vec(), i0, j0),
            }
        }
        if !self.con_letra() || i >= j {
            return (s.to_vec(), i0, j0);
        }
        // Un escape (\*), un tramo de código o una dirección no se parten: lo que los toca los coge enteros. De un enlace
        // se puede coger solo parte del texto que se ve.
        let escape = |k: usize| k > 0 && k < s.len() && es(s[k - 1], b'\\') && self.f[k - 1] & MARCA != 0;
        if escape(i) {
            i -= 1;
        }
        if escape(j) {
            j += 1;
        }
        loop {
            let (ai, aj) = (i, j);
            for q in &self.p {
                let partible = q.efecto == ENLACE && q.fin > q.cierra + 1 && q.abre + q.n <= i && j <= q.cierra;
                let entero = (q.efecto == CODIGO && efecto != CODIGO) || q.efecto == ENLACE && !partible;
                if entero && q.abre < j && i < q.fin {
                    (i, j) = (i.min(q.abre), j.max(q.fin));
                }
            }
            if (i, j) == (ai, aj) {
                break;
            }
        }
        // Y la selección queda cogiéndolos.
        let (i0, j0) = if i0 < j0 { (i0.min(i), j0.max(j)) } else { (i0, j0) };
        let afectadas: Vec<Par> = self.p.iter().copied().filter(|q| q.efecto == efecto && q.abre <= j && i <= q.fin).collect();
        let r0 = afectadas.iter().map(|q| q.abre).fold(i, usize::min);
        let r1 = afectadas.iter().map(|q| q.fin).fold(j, usize::max);
        let mut fuera = vec![false; r1 - r0]; // marcas que se quitan
        let mut con = vec![false; r1 - r0]; // lo que queda con el efecto
        for q in &afectadas {
            fuera[q.abre - r0..q.abre + q.n - r0].fill(true);
            fuera[q.cierra - r0..q.fin - r0].fill(true);
            con[q.abre + q.n - r0..q.cierra - r0].fill(true);
        }
        con[i - r0..j - r0].fill(poner);
        for k in r0..r1 {
            if self.tipo == Tipo::Tabla && es(s[k], b'|') && self.f[k] & MARCA != 0 {
                con[k - r0] = false; // las rayas de una tabla separan celdas
            }
        }
        let quedan: Vec<usize> = (r0..r1).filter(|&k| !fuera[k - r0]).collect();
        let suelta = |k: usize| es(s[k], b'\\') && self.f[k] & PROTEGIDO == 0;
        let mut nueva = s[..r0].to_vec();
        let mut antes = r0.checked_sub(1); // de dónde viene lo último puesto en la línea nueva
        let (mut ni, mut nj) = (None, None);
        let mut t = 0;
        while t < quedan.len() {
            let v = con[quedan[t] - r0];
            let mut u = t;
            while u < quedan.len() && con[quedan[u] - r0] == v {
                u += 1;
            }
            // Las marcas van pegadas al texto, y un tramo que solo tiene marcas de otras cosas no lleva.
            let (mut a, mut b) = (t, u);
            while v && a < b && espacio(s[quedan[a]]) {
                a += 1;
            }
            // Una \ suelta al final escaparía la marca de cerrar: se queda fuera.
            let barra = |k: usize| efecto != CODIGO && es(s[k], b'\\') && self.f[k] & PROTEGIDO == 0;
            while v && b > a && (espacio(s[quedan[b - 1]]) || barra(quedan[b - 1])) {
                b -= 1;
            }
            let lleva = v && quedan[a..b].iter().any(|&k| self.f[k] & MARCA == 0 && !espacio(s[k]) && !es(s[k], b'\\'));
            let (abre, cierra) = if lleva { marcas(efecto, quedan[a..b].iter().map(|&k| s[k])) } else { (Vec::new(), Vec::new()) };
            for x in t..u {
                if x == a && !abre.is_empty() {
                    // Detrás de una \ suelta, la marca de abrir quedaría escapada: la \ pasa adentro.
                    if antes.is_some_and(suelta) {
                        let barra = nueva.pop();
                        nueva.extend_from_slice(&abre);
                        nueva.extend(barra);
                    } else {
                        nueva.extend_from_slice(&abre);
                    }
                }
                if ni.is_none() && quedan[x] >= i0 {
                    ni = Some(nueva.len());
                }
                nueva.push(s[quedan[x]]);
                antes = Some(quedan[x]);
                if quedan[x] < j0 {
                    nj = Some(nueva.len());
                }
                if x + 1 == b {
                    nueva.extend_from_slice(&cierra);
                }
            }
            t = u;
        }
        let tramo = nueva.len() - r0;
        nueva.extend_from_slice(&s[r1..]);
        let fuera_de = |p: usize| if p <= r0 { p } else { p + tramo - (r1 - r0) };
        // Con todo el texto de la línea seleccionado, la selección sigue cogiendo la línea entera, marcas incluidas.
        let entera = i0 < j0 && i0 <= self.ini && j0 >= self.fin;
        let ni = if i0 < r0 || i0 >= r1 || entera { fuera_de(i0) } else { ni.unwrap_or(r0 + tramo) };
        let nj = if j0 <= r0 || j0 > r1 || entera { fuera_de(j0) } else { nj.unwrap_or(r0) };
        (nueva, ni, nj.max(ni))
    }
}

/// Las marcas de un efecto para este texto. El código lleva una comilla más que la tirada más larga de dentro, y un
/// espacio si el texto empieza o acaba por comilla.
fn marcas(efecto: u8, texto: impl Iterator<Item = u16> + Clone) -> (Vec<u16>, Vec<u16>) {
    let m = match efecto {
        NEGRITA => u("**"),
        CURSIVA => u("*"),
        TACHADO => u("~~"),
        _ => {
            let (mut largo, mut tirada) = (0, 0);
            for c in texto.clone() {
                tirada = if es(c, b'`') { tirada + 1 } else { 0 };
                largo = largo.max(tirada);
            }
            let comillas = vec![b'`' as u16; largo + 1];
            let (primero, ultimo) = (texto.clone().next(), texto.last());
            if primero.is_some_and(|c| es(c, b'`')) || ultimo.is_some_and(|c| es(c, b'`')) {
                return ([comillas.clone(), u(" ")].concat(), [u(" "), comillas].concat());
            }
            comillas
        }
    };
    (m.clone(), m)
}

/// ^ {0,3}(#{1,6})[ \t] → (nivel, largo de la marca hasta el primer espacio incluido). Quitar solo un espacio deja la
/// línea como estaba antes de ponerle el título.
pub fn prefijo_titulo(l: &[u16]) -> Option<(usize, usize)> {
    let n = sangria(l);
    let h = l[n..].iter().take_while(|&&c| es(c, b'#')).count();
    let espacio = l.get(n + h).is_some_and(|&c| es(c, b' ') || es(c, b'\t'));
    (n <= 3 && (1..=6).contains(&h) && espacio).then_some((h, n + h + 1))
}

/// ^(\s*)(?:[-*+] \[[ xX]\] |[-*+] |\d+[.)] ) → (sangría, largo de sangría y marca)
pub fn marca_lista(l: &[u16]) -> Option<(usize, usize)> {
    let n = sangria_lista(l);
    let r = &l[n..];
    if r.len() >= 2 && (es(r[0], b'-') || es(r[0], b'*') || es(r[0], b'+')) && es(r[1], b' ') {
        if r.len() >= 6 && es(r[2], b'[') && (es(r[3], b' ') || es(r[3], b'x') || es(r[3], b'X')) && es(r[4], b']') && es(r[5], b' ') {
            return Some((n, n + 6));
        }
        return Some((n, n + 2));
    }
    let d = r.iter().take_while(|&&c| (b'0' as u16..=b'9' as u16).contains(&c)).count();
    if d > 0 && r.len() >= d + 2 && (es(r[d], b'.') || es(r[d], b')')) && es(r[d + 1], b' ') {
        return Some((n, n + d + 2));
    }
    None
}

/// 0 viñetas, 1 numerada, 2 casillas, 3 cita: ¿la línea ya es de ese tipo?
pub fn es_de_tipo(l: &[u16], tipo: u8) -> bool {
    let n = sangria_lista(l);
    let r = &l[n..];
    let vineta = r.len() >= 2 && (es(r[0], b'-') || es(r[0], b'*') || es(r[0], b'+')) && es(r[1], b' ');
    let casilla = vineta && r.len() >= 6 && es(r[2], b'[') && (es(r[3], b' ') || es(r[3], b'x') || es(r[3], b'X')) && es(r[4], b']') && es(r[5], b' ');
    match tipo {
        0 => vineta && !casilla,
        1 => {
            let d = r.iter().take_while(|&&c| (b'0' as u16..=b'9' as u16).contains(&c)).count();
            d > 0 && r.len() >= d + 2 && (es(r[d], b'.') || es(r[d], b')')) && es(r[d + 1], b' ')
        }
        2 => casilla,
        _ => r.first().is_some_and(|&c| es(c, b'>')),
    }
}

/// ^(\s*)> ? → $1
pub fn quitar_cita(l: &[u16]) -> Vec<u16> {
    let n = sangria_lista(l);
    if !l.get(n).is_some_and(|&c| es(c, b'>')) {
        return l.to_vec();
    }
    let resto = if l.get(n + 1).is_some_and(|&c| es(c, b' ')) { n + 2 } else { n + 1 };
    [&l[..n], &l[resto..]].concat()
}

/// Intro en una lista: lo que va al principio de la línea nueva, o None si no es una lista. `vacio` dice si el
/// elemento no tiene nada detrás de la marca (entonces Intro la termina). `largo` es lo que mide la marca.
pub struct Seguir {
    pub siguiente: Vec<u16>,
    pub largo: usize,
}

pub fn seguir_lista(antes: &[u16]) -> Option<Seguir> {
    // ^(\s*)(?:([-*+])|(\d{1,9})([.)]))( +)(\[[ xX]\] +)?
    let n = sangria_lista(antes);
    let r = &antes[n..];
    let (marca, mut i) = if !r.is_empty() && (es(r[0], b'-') || es(r[0], b'*') || es(r[0], b'+')) {
        (vec![r[0]], 1)
    } else {
        let d = r.iter().take_while(|&&c| (b'0' as u16..=b'9' as u16).contains(&c)).count();
        if d == 0 || d > 9 || r.len() <= d || !(es(r[d], b'.') || es(r[d], b')')) {
            return None;
        }
        let num: u64 = String::from_utf16_lossy(&r[..d]).parse().ok()?;
        let mut m = u(&(num + 1).to_string());
        m.push(r[d]);
        (m, d + 1)
    };
    let e = r[i..].iter().take_while(|&&c| es(c, b' ')).count();
    if e == 0 {
        return None;
    }
    let espacios = r[i..i + e].to_vec();
    i += e;
    let mut tarea = false;
    if r.len() >= i + 3 && es(r[i], b'[') && (es(r[i + 1], b' ') || es(r[i + 1], b'x') || es(r[i + 1], b'X')) && es(r[i + 2], b']') {
        let k = r[i + 3..].iter().take_while(|&&c| es(c, b' ')).count();
        if k > 0 {
            tarea = true;
            i += 3 + k;
        }
    }
    let mut siguiente = antes[..n].to_vec();
    siguiente.extend(marca);
    siguiente.extend(espacios);
    if tarea {
        siguiente.extend(u("[ ] "));
    }
    Some(Seguir { siguiente, largo: n + i })
}

/// Si la línea puede ser de una lista: un elemento, una línea en blanco o una con sangría (lo que sigue a un elemento).
/// Una valla de código no: dentro, «1. x» no es una lista.
pub fn sigue_lista(l: &[u16]) -> bool {
    let r = &l[sangria_lista(l)..];
    let valla = r.len() >= 3 && (r[..3] == [96, 96, 96] || r[..3] == [126, 126, 126]);
    !valla && (blanca(l) || sangria_lista(l) > 0 || marca_lista(l).is_some())
}

/// El número de un elemento de lista numerada y dónde están sus cifras.
fn numero(l: &[u16]) -> Option<(u64, usize, usize)> {
    let (i, _) = marca_lista(l)?;
    let d = l[i..].iter().take_while(|&&c| (b'0' as u16..=b'9' as u16).contains(&c)).count();
    if d == 0 || d > 9 {
        return None;
    }
    Some((String::from_utf16_lossy(&l[i..i + d]).parse().ok()?, i, i + d))
}

/// Vuelve a numerar las listas numeradas desde la línea `desde`: cada elemento sigue al de encima de su mismo nivel; el
/// primero de su nivel empieza en 1 si está en `nuevas` (se acaba de mover o de crear) o cuelga de una de ellas, y si
/// no, se queda como está (una lista puede empezar en 5 a propósito). Las líneas de antes de `desde` solo dicen por
/// dónde va cada nivel.
pub fn renumerar(lineas: &mut [Vec<u16>], desde: usize, nuevas: &[usize]) {
    for i in desde..lineas.len() {
        let Some((n, a, b)) = numero(&lineas[i]) else { continue };
        let nivel = sangria_lista(&lineas[i]);
        let (mut anterior, mut padre) = (None, None);
        for j in (0..i).rev().filter(|&j| !blanca(&lineas[j])) {
            let k = sangria_lista(&lineas[j]);
            if k < nivel {
                padre = Some(j);
                break;
            }
            if k == nivel {
                anterior = numero(&lineas[j]).map(|x| x.0);
                break;
            }
        }
        let quiere = match anterior {
            Some(m) => m + 1,
            None if nuevas.contains(&i) || padre.is_some_and(|j| nuevas.contains(&j)) => 1,
            None => n,
        };
        if quiere != n {
            lineas[i].splice(a..b, u(&quiere.to_string()));
        }
    }
}

// --- Alinear tablas ---

fn blanca(s: &[u16]) -> bool {
    s.iter().all(|&c| espacio(c))
}

fn recortar(s: &[u16]) -> &[u16] {
    let a = s.iter().take_while(|&&c| espacio(c)).count();
    let b = s[a..].iter().rev().take_while(|&&c| espacio(c)).count();
    &s[a..s.len() - b]
}

/// ^ *\|? *:?-+:? *(\| *:?-+:? *)*\|? *$
/// ¿Es una fila de tabla? Empieza por | tras la sangría.
pub fn fila(l: &[u16]) -> bool {
    l.get(sangria(l)).is_some_and(|&c| es(c, b'|'))
}

/// Un |---| que se pliega si tiene una fila encima.
pub fn plegable(l: &[u16]) -> bool {
    fila(l) && separador(l)
}

fn separador(s: &[u16]) -> bool {
    let mut t = recortar(s);
    if t.first().is_some_and(|&c| es(c, b'|')) {
        t = &t[1..];
    }
    if t.last().is_some_and(|&c| es(c, b'|')) {
        t = &t[..t.len() - 1];
    }
    !t.is_empty()
        && t.split(|&c| es(c, b'|')).all(|celda| {
            let c = recortar(celda);
            let c = c.strip_prefix(&[b':' as u16]).unwrap_or(c);
            let c = c.strip_suffix(&[b':' as u16]).unwrap_or(c);
            !c.is_empty() && c.iter().all(|&x| es(x, b'-'))
        })
}

fn es_tabla(l: &[&[u16]], i: usize) -> bool {
    l[i].contains(&(b'|' as u16)) && i + 1 < l.len() && l[i + 1].contains(&(b'|' as u16)) && separador(l[i + 1])
}

/// Celdas de una fila tal cual (con \| incluido), sin espacios alrededor.
pub fn celdas(s: &[u16]) -> Vec<Vec<u16>> {
    let mut r = Vec::new();
    let mut c = Vec::new();
    let mut t = recortar(s);
    if t.first().is_some_and(|&x| es(x, b'|')) {
        t = &t[1..];
    }
    let mut cierra = false;
    let mut k = 0;
    while k < t.len() {
        let ch = t[k];
        cierra = false;
        if es(ch, b'\\') && k + 1 < t.len() && es(t[k + 1], b'|') {
            c.extend_from_slice(&t[k..k + 2]);
            k += 2;
            continue;
        }
        if es(ch, b'|') {
            r.push(recortar(&c).to_vec());
            c.clear();
            cierra = true;
        } else {
            c.push(ch);
        }
        k += 1;
    }
    let ultima = recortar(&c).to_vec();
    if !ultima.is_empty() || !cierra {
        r.push(ultima);
    }
    r
}

/// Las celdas de una fila como se leen, sin marcas ni espacios alrededor: cada letra con su formato (NEGRITA, CURSIVA,
/// TACHADO, CODIGO o ENLACE).
pub fn celdas_vistas(s: &[u16]) -> Vec<Vec<(u16, u8)>> {
    let (mut f, mut p, mut a) = (Vec::new(), Vec::new(), None);
    analizar(s, &mut a, &mut f, &mut p);
    let recortada = |c: &[(u16, u8)]| {
        let a = c.iter().take_while(|x| espacio(x.0)).count();
        let b = c[a..].iter().rev().take_while(|x| espacio(x.0)).count();
        c[a..c.len() - b].to_vec()
    };
    let (mut r, mut c, mut dentro) = (Vec::new(), Vec::new(), false);
    for (i, &ch) in s.iter().enumerate() {
        if es(ch, b'|') && f[i] & MARCA != 0 {
            if dentro {
                r.push(recortada(&c));
                c.clear();
            }
            dentro = true;
        } else if dentro && f[i] & MARCA == 0 {
            c.push((ch, f[i] & (NEGRITA | CURSIVA | TACHADO | CODIGO | ENLACE)));
        }
    }
    if c.iter().any(|x| !espacio(x.0)) {
        r.push(recortada(&c));
    }
    r
}

/// Cómo se alinea cada columna según la fila |---|: 0 a la izquierda, 1 al centro, 2 a la derecha.
pub fn alineacion(separador: &[u16]) -> Vec<u8> {
    celdas(separador)
        .iter()
        .map(|c| match (c.first().is_some_and(|&x| es(x, b':')), c.len() > 1 && c.last().is_some_and(|&x| es(x, b':'))) {
            (true, true) => 1,
            (false, true) => 2,
            _ => 0,
        })
        .collect()
}

fn raya(celda: &[u16], ancho: usize) -> Vec<u16> {
    let a = celda.first().is_some_and(|&c| es(c, b':'));
    let b = celda.len() > 1 && celda.last().is_some_and(|&c| es(c, b':'));
    let mut r = Vec::new();
    if a {
        r.push(b':' as u16);
    }
    r.extend(std::iter::repeat_n(b'-' as u16, ancho - a as usize - b as usize));
    if b {
        r.push(b':' as u16);
    }
    r
}

/// Alinea las columnas de todas las tablas para que también se lean bien como texto. None si no cambia nada.
pub fn alinear_tablas(texto: &[u16]) -> Option<Vec<u16>> {
    let l: Vec<&[u16]> = texto.split(|&c| c == CR).collect();
    let mut nuevas: Vec<Option<Vec<u16>>> = vec![None; l.len()];
    // Las vallas de código, como al pintar: una de cuatro ``` no la cierra una de tres.
    let mut codigo = None;
    let mut i = 0;
    let mut queda = MAX_ALINEADO;
    while i < l.len() {
        let t = &l[i][l[i].iter().take_while(|&&c| espacio(c)).count()..];
        let antes = codigo.is_some();
        codigo = tras_linea(l[i], codigo);
        if antes || codigo.is_some() || !es_tabla(&l, i) {
            i += 1;
            continue;
        }
        let mut j = i;
        // Una valla corta la tabla: lo de debajo es código y no se toca.
        while j < l.len() && !blanca(l[j]) && l[j].contains(&(b'|' as u16)) && valla(l[j]).is_none() {
            j += 1;
        }
        let sangria = &l[i][..l[i].len() - t.len()];
        let filas: Vec<Vec<Vec<u16>>> = (i..j).map(|k| celdas(l[k])).collect();
        let nc = filas.iter().map(Vec::len).max().unwrap_or(0);
        let mut ancho = vec![3usize; nc];
        for (f, fila) in filas.iter().enumerate() {
            if f != 1 {
                for (c, celda) in fila.iter().enumerate() {
                    ancho[c] = ancho[c].max(celda.len());
                }
            }
        }
        // Rellenar todas las filas hasta la celda más ancha puede multiplicar el texto: una cabecera enorme sobre
        // miles de filas cortas no cabría en memoria. El tope es para todas las tablas juntas; la que no cabe, como está.
        let largo = (j - i) * (ancho.iter().sum::<usize>() + 3 * nc + 2);
        if largo > queda {
            i = j;
            continue;
        }
        queda -= largo;
        for (f, fila) in filas.iter().enumerate() {
            let mut b = sangria.to_vec();
            b.push(b'|' as u16);
            for (c, &a) in ancho.iter().enumerate() {
                let celda = fila.get(c).map(Vec::as_slice).unwrap_or(&[]);
                b.push(b' ' as u16);
                if f == 1 {
                    b.extend(raya(celda, a));
                } else {
                    b.extend_from_slice(celda);
                    b.extend(std::iter::repeat_n(b' ' as u16, a - celda.len()));
                }
                b.extend(u(" |"));
            }
            if b != l[i + f] {
                nuevas[i + f] = Some(b);
            }
        }
        i = j;
    }
    if nuevas.iter().all(Option::is_none) {
        return None;
    }
    let mut r = Vec::with_capacity(texto.len() + 64);
    for (k, linea) in l.iter().enumerate() {
        if k > 0 {
            r.push(CR);
        }
        r.extend_from_slice(nuevas[k].as_deref().unwrap_or(linea));
    }
    Some(r)
}

#[cfg(test)]
mod pruebas {
    use super::*;

    const COLORES: Colores = Colores { texto: 1, apagado: 2, acento: 3, fondo_codigo: 4, cita: 5, fondo: 6, rejilla: 7, raya: 8, fondo_cita: 9, barra_cita: 10 };

    // Lo que queda del RTF como texto: así se comprueba que el formato nunca cambia lo escrito.
    fn texto_de(rtf: &[u8]) -> Vec<u16> {
        let s = std::str::from_utf8(rtf).unwrap();
        let s = &s[s.find("\\cf1 ").unwrap() + 5..s.len() - 1];
        let b = s.as_bytes();
        let mut r = Vec::new();
        let mut i = 0;
        while i < b.len() {
            match b[i] {
                b'{' | b'}' => i += 1,
                b'\\' => {
                    let n = b[i + 1];
                    if n == b'\\' || n == b'{' || n == b'}' {
                        r.push(n as u16);
                        i += 2;
                        continue;
                    }
                    let mut k = i + 1;
                    while k < b.len() && b[k].is_ascii_alphabetic() {
                        k += 1;
                    }
                    let palabra = &s[i + 1..k];
                    let mut m = k;
                    while m < b.len() && (b[m] == b'-' || b[m].is_ascii_digit()) {
                        m += 1;
                    }
                    let num = &s[k..m];
                    match palabra {
                        "par" => r.push(CR),
                        "tab" => r.push(9),
                        "u" => {
                            r.push(num.parse::<i16>().unwrap() as u16);
                            m += 1; // el ? de después
                        }
                        _ => {}
                    }
                    if m < b.len() && b[m] == b' ' && palabra != "u" {
                        m += 1;
                    }
                    i = m;
                }
                c => {
                    r.push(c as u16);
                    i += 1;
                }
            }
        }
        r
    }

    fn ida_y_vuelta(t: &str) {
        let v = u(t);
        for ocultar in [false, true] {
            let rtf = pintar(&COLORES, &v, 0, v.len(), None, ocultar, false, &[], false).rtf;
            assert_eq!(texto_de(&rtf), v, "el RTF guarda el texto: {t:?}");
        }
    }

    #[test]
    fn el_formato_no_cambia_el_texto() {
        for t in [
            "", "a", "a\r", "a\r\r", "\r", "# Título *uno*", "Texto con **negrita**, `código` y [enlace](https://ejemplo.es).",
            "| Nombre | Importe |\r|:--|--:|\r| Luis \\| Pepe | 20€ |", "- uno\r  - dos\r- [x] hecha\r1. primero",
            "> cita **fuerte**\r```\r<b>sin tocar</b> {llaves} \\barra\r```", "\ttab y emoji 😀 y ñ", "***x*** __a__ _b_ ~~c~~",
            "[a](b(c)) ![i](x.png) <https://x.y> www.ejemplo.es", "\\* no es cursiva \\", "{\\rtf1 inyección}",
        ] {
            ida_y_vuelta(t);
        }
    }

    #[test]
    fn parrafos_con_el_formato_final() {
        let t = u("- uno\r  - dos\r- [ ] tarea\r- [x] hecha\r12) doce\r> cita\r> > dentro\r| a | b |\r|---|---|\r| 1 | 2 |\r---\rtexto");
        let p = pintar(&COLORES, &t, 0, t.len(), None, true, false, &[], false);
        let vistos: Vec<(usize, u16, i32)> = p.parrafos.iter().map(|&(pos, x)| (pos, x.numero, x.sangria)).collect();
        assert_eq!(
            vistos,
            [(0, 0x2022, SANGRIA - VINETA), (6, 0x25E6, 2 * SANGRIA - VINETA), (14, 0x2610, SANGRIA / 2), (26, 0x2611, SANGRIA / 2), (38, NUMERADA, SANGRIA / 2), (47, 0, CITA), (54, 0, 2 * CITA), (75, 0, 0), (95, 0, 0)]
        );
        let (_, doce) = p.parrafos[4];
        assert!(doce.inicio == 12 && doce.estilo == NUEVO_NUMERO, "«12)»: el 12, con paréntesis");
        assert!(p.parrafos[7].1.plegado && p.parrafos[8].1.raya, "|---| plegada y la raya, raya");
        let rtf = String::from_utf8(p.rtf).unwrap();
        assert!(rtf.contains("\\cf1\\b  a }") && !rtf.contains("\\b  1 "), "la cabecera de la tabla, en negrita: {rtf}");
        let con_marcas = pintar(&COLORES, &t, 0, t.len(), None, false, false, &[], false).parrafos;
        assert_eq!(con_marcas.iter().map(|&(pos, _)| pos).collect::<Vec<_>>(), [95], "con las marcas a la vista solo cambia la raya");
    }

    fn marcas(t: &str) -> Vec<u8> {
        let s = u(t);
        let mut f = vec![0; s.len()];
        en_linea(&s, &mut f, &mut Vec::new(), 0, s.len());
        f
    }

    /// Pulsa el botón de un efecto con t[i..j] seleccionado, como la barra: lo quita si ya lo tiene todo.
    fn pulsar(t: &str, i: usize, j: usize, efecto: u8) -> (String, String) {
        let s = u(t);
        let l = Linea::leer(&s, None);
        let poner = l.tiene(&s, i, j, efecto) != Some(true);
        let (n, a, b) = l.alternar(&s, i, j, efecto, poner);
        (String::from_utf16_lossy(&n), String::from_utf16_lossy(&n[a..b]))
    }

    #[test]
    fn pone_y_quita_formato() {
        let casos: [(&str, usize, usize, u8, &str, &str); 14] = [
            ("palabra", 0, 3, NEGRITA, "**pal**abra", "pal"),
            ("una palabra", 4, 11, NEGRITA, "una **palabra**", "palabra"),
            ("a **bc** d", 4, 6, NEGRITA, "a bc d", "bc"),
            ("**negrita**", 2, 9, CURSIVA, "***negrita***", "negrita"),
            ("***negrita***", 3, 10, CURSIVA, "**negrita**", "negrita"),
            ("**negrita**", 2, 5, NEGRITA, "neg**rita**", "neg"),
            ("x **negrita** y", 5, 9, NEGRITA, "x **n**egri**ta** y", "egri"),
            ("*a*b", 3, 4, CURSIVA, "*ab*", "b"),
            ("a **bc** d", 5, 5, NEGRITA, "a bc d", ""),
            ("a `b` c", 3, 4, CODIGO, "a b c", "b"),
            ("x`y", 0, 3, CODIGO, "``x`y``", "``x`y``"),
            ("| a | b |", 0, 9, NEGRITA, "| **a** | **b** |", "| **a** | **b** |"),
            ("- item", 0, 6, NEGRITA, "- **item**", "- **item**"),
            ("a **b** c", 0, 9, NEGRITA, "**a b c**", "**a b c**"),
        ];
        for (t, i, j, efecto, linea, sel) in casos {
            assert_eq!(pulsar(t, i, j, efecto), (linea.to_string(), sel.to_string()), "{t:?} [{i}..{j}]");
        }
        let (n, _) = pulsar("a **bc** d", 5, 5, NEGRITA);
        assert_eq!(n, "a bc d", "con el cursor dentro se quita la pareja entera");
        assert_eq!(pulsar("```", 0, 3, NEGRITA).0, "```", "una valla no lleva formato");
        assert_eq!(pulsar("# Título", 0, 8, NEGRITA).0, "# **Título**", "la marca del título queda fuera");
        let (puesto, _) = pulsar("`x", 0, 2, CODIGO);
        assert_eq!(puesto, "`` `x ``");
        assert_eq!(pulsar(&puesto, 3, 5, CODIGO).0, "`x", "el relleno se va con las comillas");
    }

    #[test]
    fn lineas_que_siguen_un_elemento() {
        let t = u("- uno\r  sigue\rpegada\r\r  tras blanca\r\rfuera\r1. diez\r   del diez");
        let todo = pintar(&COLORES, &t, 0, t.len(), None, true, false, &[], false).parrafos;
        let sangrias: Vec<(usize, i32)> = todo.iter().map(|&(pos, x)| (pos, x.sangria)).collect();
        assert_eq!(sangrias, [(0, SANGRIA - VINETA), (6, SANGRIA), (14, SANGRIA), (22, SANGRIA), (43, SANGRIA / 2), (51, SANGRIA)], "{todo:?}");
        // Pintada desde cualquier línea, cada una queda igual que pintando todo.
        let mut ini = 0;
        for l in t.split(|&c| c == CR) {
            let desde: Vec<(usize, Parrafo)> = pintar(&COLORES, &t, ini, t.len(), None, true, false, &[], false).parrafos.iter().map(|&(p, x)| (p + ini, x)).collect();
            let esperado: Vec<(usize, Parrafo)> = todo.iter().copied().filter(|&(p, _)| p >= ini).collect();
            assert_eq!(desde, esperado, "desde {ini}");
            ini += l.len() + 1;
        }
        let rtf = String::from_utf8(pintar(&COLORES, &t, 6, 13, None, true, false, &[], false).rtf).unwrap();
        assert!(rtf.contains("\\fs2\\cf"), "los espacios de delante se esconden: {rtf}");
    }

    #[test]
    fn revision_fable() {
        let vistos = |t: &[u16], ini: usize| -> Vec<(usize, u16, i32, bool)> {
            pintar(&COLORES, t, ini, t.len(), None, true, false, &[], false).parrafos.iter().map(|&(pos, x)| (pos, x.numero, x.sangria, x.plegado)).collect()
        };
        let t = u("- a\r\t- b\r\t\t- c");
        assert_eq!(vistos(&t, 0), [(0, 0x2022, SANGRIA - VINETA, false), (4, 0x25E6, 2 * SANGRIA - VINETA, false), (9, 0x25AA, 3 * SANGRIA - VINETA, false)], "anidadas con tabuladores");
        assert_eq!(marca_lista(&u("\u{a0}- x")), None, "un espacio duro no es sangría");
        let t = u("|---|\rtexto\r\r|---|\r| a |\r|---|");
        assert_eq!(vistos(&t, 0), [(25, 0, 0, true)], "solo se pliega el |---| que tiene una fila encima");
        assert_eq!(vistos(&t, 25), [(0, 0, 0, true)], "también pintando solo esa línea");
        let t = u(&(">".repeat(40) + " x"));
        assert_eq!(vistos(&t, 0)[0].2, 8 * CITA, "la sangría de una cita tiene tope");
    }

    #[test]
    fn revision_astra() {
        let l = |s: &str| marcas(s);
        // Una valla no cierra con las comillas separadas ni con cuatro espacios delante.
        let t = u("```\n` ` `\n    ```\nsigue");
        let mut a = None;
        for linea in t.split(|&c| c == b'\n' as u16) {
            a = tras_linea(linea, a);
        }
        assert!(a.is_some(), "el bloque sigue abierto");
        // Un paréntesis escapado es parte de la dirección.
        assert!(l("[x](a\\)b)").iter().all(|&f| f & ENLACE != 0 || f & MARCA != 0), "toda la línea es el enlace");
        // Muchos «<http://» sin cerrar: tiene que acabar enseguida.
        let t = "<http://".repeat(2500);
        let ahora = std::time::Instant::now();
        l(&t);
        assert!(ahora.elapsed().as_millis() < 200, "{:?}", ahora.elapsed());
        // Alinear no se mete en el bloque de código que sigue a una tabla.
        let t = u("|a|\r|-|\r```rust|x\r|b|\r```");
        assert_eq!(String::from_utf16_lossy(&alinear_tablas(&t).unwrap()), "| a   |\r| --- |\r```rust|x\r|b|\r```");
    }

    #[test]
    fn poner_y_quitar_deja_el_texto_como_estaba() {
        let trozos = ["a", "b", " ", "*", "**", "_", "~~", "`", "[x](y)", "\\", "| ", "# ", "- ", "palabra", "ñ"];
        let mut semilla: u64 = 11;
        let mut azar = |n: usize| {
            semilla = semilla.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            ((semilla >> 33) % n as u64) as usize
        };
        for _ in 0..20000 {
            let t: String = (0..1 + azar(8)).map(|_| trozos[azar(trozos.len())]).collect();
            let s = u(&t);
            let (i, j) = { let (a, b) = (azar(s.len() + 1), azar(s.len() + 1)); (a.min(b), a.max(b)) };
            let efecto = [NEGRITA, CURSIVA, TACHADO, CODIGO][azar(4)];
            let l = Linea::leer(&s, None);
            let poner = l.tiene(&s, i, j, efecto) != Some(true);
            let (n, a, b) = l.alternar(&s, i, j, efecto, poner);
            assert!(a <= b && b <= n.len(), "{t:?} [{i}..{j}]");
            // Lo que no es marca nunca cambia: el formato solo pone y quita marcas.
            let letras = |v: &[u16]| v.iter().filter(|&&c| !matches!(c, 0x2A | 0x5F | 0x7E | 0x60 | 0x20)).count();
            assert_eq!(letras(&n), letras(&s), "{t:?} [{i}..{j}] → {:?}", String::from_utf16_lossy(&n));
            // Si las marcas solo se pusieron, quitarlas deja el texto de antes.
            if poner && n != s && !t.contains(['*', '_', '~', '`']) && a < b {
                let l2 = Linea::leer(&n, None);
                assert_eq!(l2.tiene(&n, a, b, efecto), Some(true), "{t:?} [{i}..{j}] → {:?}", String::from_utf16_lossy(&n));
                let (m, _, _) = l2.alternar(&n, a, b, efecto, false);
                assert_eq!(String::from_utf16_lossy(&m), t, "[{i}..{j}] efecto {efecto}");
            }
        }
    }

    #[test]
    fn reconoce_el_formato() {
        let f = marcas("a **b** c");
        assert!(f[2] & MARCA != 0 && f[4] & NEGRITA != 0 && f[5] & MARCA != 0 && f[8] == 0, "negrita");
        let f = marcas("***x***");
        assert!(f[3] & NEGRITA != 0 && f[3] & CURSIVA != 0 && f.iter().enumerate().all(|(i, &x)| i == 3 || x & MARCA != 0), "negrita y cursiva");
        let f = marcas("a*b* y snake_case_name");
        assert!(f[2] & CURSIVA != 0 && f[13..].iter().all(|&x| x == 0), "_ dentro de palabra no es cursiva");
        let f = marcas("`**no**`");
        assert!(f[3] & CODIGO != 0 && f[3] & NEGRITA == 0, "dentro del código no hay negrita");
        let f = marcas("[ver](https://a_b_c.es)");
        assert!(f[1] & ENLACE != 0 && f[10..].iter().all(|&x| x & MARCA != 0), "enlace con la dirección como marca");
        let f = marcas("*www.a.es* y ~~https://b.es~~");
        assert!(f[1] & CURSIVA != 0 && f[9] & MARCA != 0 && f[17] & TACHADO != 0 && f[27] & MARCA != 0, "formato alrededor de una dirección suelta");
        let f = marcas("2 * 3 * 4");
        assert!(f.iter().all(|&x| x == 0), "asteriscos sueltos no son cursiva");
        let t = u("```rust");
        assert!(valla(&t).is_some() && estado_tras(&u("```\rcódigo"), None).is_some() && estado_tras(&u("```\rx\r```"), None).is_none(), "vallas");
        assert!(titulo(&u("## Dos ##")) == Some((2, 3, 6)) && titulo(&u("#no")).is_none(), "títulos");
    }

    #[test]
    fn lineas_raras_no_cuelgan() {
        let raros = [
            "*".repeat(20000) + "a", "_a".repeat(10000), "[".repeat(20000) + "](x)", "`".repeat(20000), "| ".repeat(10000),
            "**a ".repeat(5000), ">".repeat(5000) + " x", "[a](b) ".repeat(5000), "http://x.es/".repeat(5000), "~~a".repeat(8000),
        ];
        for r in raros {
            let v = u(&r);
            let t = std::time::Instant::now();
            let rtf = pintar(&COLORES, &v, 0, v.len(), None, false, false, &[], false).rtf;
            assert!(t.elapsed().as_millis() < 300, "{} ms", t.elapsed().as_millis());
            assert_eq!(texto_de(&rtf), v);
        }
    }

    #[test]
    fn alinea_tablas() {
        let tabla = u("Antes\r| a | importe |\r|:-|-:|\r| texto largo | 5 |\rDespués");
        let alineada = alinear_tablas(&tabla).unwrap();
        assert_eq!(String::from_utf16_lossy(&alineada), "Antes\r| a           | importe |\r| :---------- | ------: |\r| texto largo | 5       |\rDespués");
        assert!(alinear_tablas(&alineada).is_none(), "alinear dos veces no cambia nada");
        let codigo = u("````\r```\r|a|b|\r|-|-|\r````");
        assert!(alinear_tablas(&codigo).is_none(), "una valla de tres no cierra una de cuatro: la tabla sigue siendo código");
        assert!(alinear_tablas(&codigo[9..20]).is_some(), "fuera del código sí se alinea");
    }

    #[test]
    fn listas() {
        let s = seguir_lista(&u("  3. [x] algo")).unwrap();
        assert_eq!(String::from_utf16_lossy(&s.siguiente), "  4. [ ] ");
        assert!(seguir_lista(&u("texto")).is_none() && seguir_lista(&u("-sin espacio")).is_none());
        assert_eq!(marca_lista(&u("- [ ] x")), Some((0, 6)));
        assert!(es_de_tipo(&u("- x"), 0) && !es_de_tipo(&u("- [ ] x"), 0) && es_de_tipo(&u("1) x"), 1) && es_de_tipo(&u(" > x"), 3));
        assert_eq!(quitar_cita(&u("  > hola")), u("  hola"));
        assert_eq!(prefijo_titulo(&u("### x")), Some((3, 4)));
        assert_eq!(prefijo_titulo(&u("  #  x")), Some((1, 4)), "quitar el título deja el espacio de más");
    }

    #[test]
    fn renumera_listas() {
        let numera = |texto: &str, desde: usize, nuevas: &[usize]| {
            let mut ls: Vec<Vec<u16>> = texto.split('\n').map(u).collect();
            renumerar(&mut ls, desde, nuevas);
            ls.iter().map(|l| String::from_utf16_lossy(l)).collect::<Vec<_>>().join("\n")
        };
        // El segundo baja un nivel: empieza la sublista en 1 y el de fuera sigue en 2.
        assert_eq!(numera("1. a\n   2. b\n3. c", 1, &[1]), "1. a\n   1. b\n2. c");
        // Otro más a la misma sublista sigue su número.
        assert_eq!(numera("1. a\n   1. b\n   2. c\n3. d", 2, &[2]), "1. a\n   1. b\n   2. c\n2. d");
        // Sube un nivel: sigue al de fuera y los de dentro que quedan empiezan en 1.
        assert_eq!(numera("1. a\n   1. b\n2. c\n   3. d", 2, &[2]), "1. a\n   1. b\n2. c\n   1. d");
        // Intro en medio: los de detrás corren uno.
        assert_eq!(numera("1. a\n2. nuevo\n2. b\n3. c", 1, &[1]), "1. a\n2. nuevo\n3. b\n4. c");
        // Una lista que empieza en 5 a propósito se queda, con líneas en blanco y continuaciones en medio.
        assert_eq!(numera("5. a\n   sigue\n\n7. b", 0, &[]), "5. a\n   sigue\n\n6. b");
        // Las viñetas no se tocan, y una viñeta del mismo nivel corta la numeración.
        assert_eq!(numera("1. a\n- b\n5. c", 0, &[]), "1. a\n- b\n5. c");
    }

    #[test]
    fn estres_al_azar() {
        let trozos = [
            "# ", "## ", "- ", "  - ", "1. ", "2) ", "* ", "| ", " | ", "|---|", "|:-:|", "---", "```", "~~~", "> ", ">> ", "[x] ", "**", "*",
            "_", "__", "~~", "`", "``", "[a](b)", "[", "]", "(", ")", "![i](x.png)", "\\", "\\|", "  ", "\t", "\r", "\r\r", "texto", "año",
            "12", "www.ejemplo.es", "http://a.b/c(d)", "<http://x.y>", "{", "}", "😀",
        ];
        let mut semilla: u64 = 7;
        let mut azar = |n: usize| {
            semilla = semilla.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            ((semilla >> 33) % n as u64) as usize
        };
        for _ in 0..3000 {
            let doc: String = (0..1 + azar(150)).map(|_| trozos[azar(trozos.len())]).collect();
            let v = u(&doc);
            let p = pintar(&COLORES, &v, 0, v.len(), None, azar(2) == 0, false, &[], false);
            assert_eq!(texto_de(&p.rtf), v, "{doc:?}");
            let mut hasta = 0;
            for (i, t) in p.tramos.iter().enumerate() {
                assert!(t.0 == hasta && t.1 > t.0 && (i == 0 || p.tramos[i - 1].2 != t.2), "tramos seguidos y juntados: {doc:?}");
                hasta = t.1;
            }
            assert_eq!(hasta, v.len(), "los tramos cubren todo lo pintado: {doc:?}");
            if let Some(a) = alinear_tablas(&v) {
                assert!(alinear_tablas(&a).is_none(), "alinear es estable: {doc:?}");
                assert_eq!(a.iter().filter(|&&c| c == CR).count(), v.iter().filter(|&&c| c == CR).count());
            }
        }
    }
}
