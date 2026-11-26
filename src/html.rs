// De HTML (lo que copian los navegadores, Word, Outlook o Google Docs) a Markdown, para pegar con formato en los .md.
// Solo lo que Markdown sabe decir: títulos, negrita, cursiva, tachado, código, enlaces, imágenes, listas, citas, tablas
// y rayas. Colores, letras y demás estilos se pierden y queda el texto.

/// Más hondo no se anida: un HTML raro no puede agotar la pila al recorrerlo.
const PROFUNDIDAD: usize = 100;

const VACIOS: &[&str] = &["br", "img", "hr", "meta", "link", "input", "col", "wbr", "area", "base", "source", "param", "embed", "track"];
const SALTAR: &[&str] = &["script", "style", "head", "title", "xml", "template", "textarea", "noscript", "svg", "math", "object", "iframe", "select"];
const BLOQUES: &[&str] = &[
    "p", "div", "h1", "h2", "h3", "h4", "h5", "h6", "ul", "ol", "li", "blockquote", "pre", "table", "hr", "section", "article",
    "header", "footer", "nav", "aside", "main", "figure", "figcaption", "dl", "dt", "dd", "address", "center", "details",
    "summary", "form", "fieldset", "body", "html", "caption", "legend", "tbody", "thead", "tfoot", "tr", "td", "th",
];

enum Nodo {
    Texto(String),
    Elem(Elem),
    /// La viñeta o el número que Word escribe a mano delante de cada elemento de sus listas.
    MarcaWord(String),
}

struct Elem {
    nombre: String,
    atributos: String,
    hijos: Vec<Nodo>,
}

/// El trozo copiado, del formato «HTML Format» del portapapeles: lo que va entre StartFragment y EndFragment.
pub fn fragmento(datos: &[u8]) -> String {
    let datos = &datos[..datos.iter().position(|&b| b == 0).unwrap_or(datos.len())];
    let cabecera = &datos[..datos.iter().position(|&b| b == b'<').unwrap_or(datos.len())];
    let cabecera = String::from_utf8_lossy(cabecera);
    let numero = |clave: &str| -> Option<usize> {
        let i = cabecera.find(clave)? + clave.len();
        let cifras: String = cabecera[i..].chars().take_while(char::is_ascii_digit).collect();
        cifras.parse().ok()
    };
    let trozo = match (numero("StartFragment:"), numero("EndFragment:")) {
        (Some(a), Some(b)) if a <= b && b <= datos.len() => &datos[a..b],
        _ => &datos[cabecera.len().min(datos.len())..],
    };
    String::from_utf8_lossy(trozo).into_owned()
}

/// Lo que se pega en un Markdown, o None para pegar el texto tal cual. `html`, el del portapapeles (si lo trae);
/// `texto`, su texto; `lengua`, la del editor de código que lo copió. Con `true`, es un bloque y va en sus líneas.
pub fn pegado(html: Option<&str>, texto: Option<&str>, lengua: &str) -> Option<(String, bool)> {
    let Some(html) = html else {
        return texto.and_then(tabla_de_tabuladores).map(|t| (t, true));
    };
    let nodos = arbol(html);
    // Lo copiado de un editor de código (todo en un bloque white-space: pre): su texto, con sus espacios, en un bloque de
    // código si es de varias líneas, aunque el tema ponga los comentarios en cursiva.
    let primero = nodos.iter().find_map(|n| match n {
        Nodo::Elem(e) if e.nombre != "meta" => Some(e),
        _ => None,
    });
    if primero.is_some_and(|e| atributo(&e.atributos, "style").is_some_and(|s| s.replace(' ', "").to_ascii_lowercase().contains("white-space:pre"))) {
        return texto.and_then(|t| bloque_de_codigo(t, lengua)).map(|b| (b, true));
    }
    convertir(&nodos).map(|m| (m, false))
}

/// El Markdown de un HTML, o None si no trae nada que Markdown sepa decir: entonces vale más el texto tal cual.
#[cfg(test)]
fn a_markdown(html: &str) -> Option<String> {
    pegado(Some(html), None, "").map(|x| x.0)
}

/// Varias líneas, en un bloque de código con su lengua; una sola (o un trozo de una), None: se pega tal cual.
fn bloque_de_codigo(texto: &str, lengua: &str) -> Option<String> {
    let t = texto.replace("\r\n", "\n").replace('\r', "\n");
    let t = t.trim_end_matches('\n');
    if !t.contains('\n') || t.trim().is_empty() {
        return None;
    }
    // Más comillas que la tirada más larga de dentro: así ninguna línea del código cierra el bloque.
    let valla = "`".repeat((tirada(t, '`') + 1).max(3));
    Some(format!("{valla}{lengua}\n{t}\n{valla}"))
}

/// La lengua que VS Code deja en «vscode-editor-data» (su «mode»), con el nombre que usan los bloques de código.
pub fn lengua_de_vscode(datos: &str) -> String {
    let Some(i) = datos.find("\"mode\"") else { return String::new() };
    let resto = datos[i + 6..].trim_start().trim_start_matches(':').trim_start().trim_start_matches('"');
    let modo: String = resto.chars().take_while(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '#' | '-' | '_')).collect();
    match modo.as_str() {
        "plaintext" => "",
        "typescriptreact" => "tsx",
        "javascriptreact" => "jsx",
        "shellscript" => "bash",
        "jsonc" => "json",
        otro => otro,
    }
    .to_string()
}

/// Texto con tabuladores (de un programa que no da formato), en tabla: dos líneas o más, todas con los mismos (uno o
/// más) y ninguna empezando por uno, que sería código sangrado. La primera, de cabecera.
fn tabla_de_tabuladores(texto: &str) -> Option<String> {
    let t = texto.replace("\r\n", "\n").replace('\r', "\n");
    let lineas: Vec<&str> = t.split('\n').filter(|l| !l.trim().is_empty()).collect();
    let n = lineas.first()?.matches('\t').count();
    if lineas.len() < 2 || n == 0 || lineas.iter().any(|l| l.starts_with('\t') || l.matches('\t').count() != n) {
        return None;
    }
    let fila = |l: &str| format!("| {} |", l.split('\t').map(|c| celda(c.trim())).collect::<Vec<_>>().join(" | "));
    let mut r = vec![fila(lineas[0]), format!("|{}", " --- |".repeat(n + 1))];
    r.extend(lineas[1..].iter().map(|l| fila(l)));
    Some(r.join("\n"))
}

fn convertir(nodos: &[Nodo]) -> Option<String> {
    let mut c = Conv { formato: false };
    let md = c.bloques(nodos, Ctx::default(), false);
    let md = md.trim_matches('\n');
    (c.formato && !md.trim().is_empty()).then(|| md.to_string())
}

// --- Leer el HTML ---

fn arbol(html: &str) -> Vec<Nodo> {
    let mut pila = vec![Elem { nombre: String::new(), atributos: String::new(), hijos: Vec::new() }];
    let mut marca: Option<String> = None;
    let b = html.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] != b'<' {
            let fin = html[i..].find('<').map_or(b.len(), |k| i + k);
            let t = decodificar(&html[i..fin]);
            match &mut marca {
                Some(m) => m.push_str(&t),
                None => meter(&mut pila, Nodo::Texto(t)),
            }
            i = fin;
            continue;
        }
        let resto = &html[i..];
        if resto.starts_with("<!--") {
            i = resto.find("-->").map_or(b.len(), |k| i + k + 3);
            continue;
        }
        if resto.starts_with("<![") {
            // Lo que Word pone para los programas que no saben de sus listas: la viñeta escrita a mano.
            let (dentro, fin) = match resto.find("]>") {
                Some(k) => (resto[3..k.max(3)].to_ascii_lowercase(), i + k + 2),
                None => (String::new(), b.len()),
            };
            if dentro.trim_start().starts_with("if") && dentro.contains("supportlists") {
                marca = Some(String::new());
            } else if dentro.trim_start().starts_with("endif")
                && let Some(m) = marca.take()
            {
                meter(&mut pila, Nodo::MarcaWord(m));
            }
            i = fin;
            continue;
        }
        if resto.starts_with("<!") || resto.starts_with("<?") {
            i = resto.find('>').map_or(b.len(), |k| i + k + 1);
            continue;
        }
        let cierra = resto.starts_with("</");
        let ini = i + if cierra { 2 } else { 1 };
        let largo = b[ini..].iter().take_while(|c| c.is_ascii_alphanumeric() || **c == b':' || **c == b'-').count();
        if largo == 0 {
            meter(&mut pila, Nodo::Texto("<".into()));
            i += 1;
            continue;
        }
        let nombre = html[ini..ini + largo].to_ascii_lowercase();
        let fin = fin_de_etiqueta(b, ini + largo);
        let atributos = html[ini + largo..fin].trim_end_matches('>').trim_end_matches('/');
        i = fin;
        // Una viñeta de Word sin su cierre (un trozo cortado) no se lleva el resto: al cambiar de bloque, es texto.
        if BLOQUES.contains(&nombre.as_str())
            && let Some(m) = marca.take()
        {
            meter(&mut pila, Nodo::Texto(m));
        }
        if cierra {
            cerrar(&mut pila, &nombre);
        } else if nombre == "math"
            && let Some(tex) = tex_de(&html[i..buscar_cierre(html, i, "math")])
        {
            i = buscar_cierre(html, i, "math");
            meter(&mut pila, Nodo::Elem(Elem { nombre, atributos: atributos.to_string(), hijos: vec![Nodo::Texto(tex)] }));
        } else if atributo(atributos, "aria-hidden").as_deref() == Some("true") && pila.last().is_some_and(|p| p.hijos.iter().any(con_formula)) {
            // La fórmula dibujada (katex-html) que va tras la de MathML: ya está en TeX.
            i = cierre_anidado(html, i, &nombre);
        } else if SALTAR.contains(&nombre.as_str()) {
            i = buscar_cierre(html, i, &nombre);
        } else {
            abrir(&mut pila, nombre, atributos.to_string());
        }
    }
    if let Some(m) = marca {
        meter(&mut pila, Nodo::Texto(m));
    }
    while pila.len() > 1 {
        cerrar_ultimo(&mut pila);
    }
    pila.pop().map(|e| e.hijos).unwrap_or_default()
}

/// Dónde acaba una etiqueta (tras su «>»), sin hacer caso de los «>» de dentro de comillas.
fn fin_de_etiqueta(b: &[u8], mut i: usize) -> usize {
    let mut comilla = 0u8;
    while i < b.len() {
        match b[i] {
            c if comilla != 0 => {
                if c == comilla {
                    comilla = 0;
                }
            }
            b'"' | b'\'' => comilla = b[i],
            b'>' => return i + 1,
            _ => {}
        }
        i += 1;
    }
    b.len()
}

/// Tras el cierre de `nombre` (sin distinguir mayúsculas), o el final si no lo tiene.
fn buscar_cierre(html: &str, desde: usize, nombre: &str) -> usize {
    let b = html.as_bytes();
    let aguja = format!("</{nombre}");
    let n = aguja.len();
    (desde..b.len().saturating_sub(n - 1))
        .find(|&k| b[k..k + n].eq_ignore_ascii_case(aguja.as_bytes()))
        .map_or(b.len(), |k| fin_de_etiqueta(b, k + n))
}

/// El TeX de una fórmula de MathML, de su `<annotation encoding="application/x-tex">`.
fn tex_de(math: &str) -> Option<String> {
    let minus = math.to_ascii_lowercase();
    let ini = minus.find("<annotation")?;
    let etiqueta = fin_de_etiqueta(minus.as_bytes(), ini);
    if !minus[ini..etiqueta].contains("application/x-tex") {
        return None;
    }
    let fin = minus[etiqueta..].find("</annotation").map_or(math.len(), |k| etiqueta + k);
    let tex = decodificar(&math[etiqueta..fin]).trim().to_string();
    (!tex.is_empty()).then_some(tex)
}

fn con_formula(n: &Nodo) -> bool {
    matches!(n, Nodo::Elem(e) if e.nombre == "math" || e.hijos.iter().any(con_formula))
}

/// Tras el cierre que empareja con la etiqueta `nombre` que acaba de abrirse (con las de dentro del mismo nombre).
fn cierre_anidado(html: &str, desde: usize, nombre: &str) -> usize {
    let b = html.as_bytes();
    let (abre, cierra) = (format!("<{nombre}"), format!("</{nombre}"));
    let mut nivel = 1;
    let mut k = desde;
    while k < b.len() {
        let es = |a: &str| b[k..].len() >= a.len() && b[k..k + a.len()].eq_ignore_ascii_case(a.as_bytes()) && b.get(k + a.len()).is_none_or(|c| !c.is_ascii_alphanumeric());
        if es(&cierra) {
            nivel -= 1;
            if nivel == 0 {
                return fin_de_etiqueta(b, k + cierra.len());
            }
        } else if es(&abre) {
            nivel += 1;
        }
        k += 1;
    }
    b.len()
}

fn meter(pila: &mut [Elem], n: Nodo) {
    if let Some(e) = pila.last_mut() {
        e.hijos.push(n);
    }
}

fn cerrar_ultimo(pila: &mut Vec<Elem>) {
    if let Some(e) = pila.pop() {
        meter(pila, Nodo::Elem(e));
    }
}

/// Cierra hasta el último `nombre` abierto (y lo que tenga dentro sin cerrar); si no hay ninguno, nada.
fn cerrar(pila: &mut Vec<Elem>, nombre: &str) {
    if let Some(j) = pila.iter().rposition(|e| e.nombre == nombre)
        && j > 0
    {
        while pila.len() > j {
            cerrar_ultimo(pila);
        }
    }
}

/// El último `nombre` abierto después del último de `dentro_de`, si lo hay.
fn abierto_en(pila: &[Elem], nombre: &[&str], dentro_de: &[&str]) -> Option<usize> {
    let j = pila.iter().rposition(|e| nombre.contains(&e.nombre.as_str()))?;
    let k = pila.iter().rposition(|e| dentro_de.contains(&e.nombre.as_str())).unwrap_or(0);
    (j > k).then_some(j)
}

fn abrir(pila: &mut Vec<Elem>, nombre: String, atributos: String) {
    // Lo que HTML cierra solo: un elemento de lista, una celda o una fila al empezar otro, y un párrafo al empezar un
    // bloque.
    let previo = match nombre.as_str() {
        "li" => abierto_en(pila, &["li"], &["ul", "ol"]),
        "td" | "th" => abierto_en(pila, &["td", "th"], &["tr", "table"]),
        "tr" => abierto_en(pila, &["tr"], &["table"]),
        n if BLOQUES.contains(&n) && pila.last().is_some_and(|e| e.nombre == "p") => Some(pila.len() - 1),
        _ => None,
    };
    if let Some(j) = previo {
        while pila.len() > j {
            cerrar_ultimo(pila);
        }
    }
    let e = Elem { nombre, atributos, hijos: Vec::new() };
    if VACIOS.contains(&e.nombre.as_str()) {
        meter(pila, Nodo::Elem(e));
    } else if pila.len() < PROFUNDIDAD {
        pila.push(e);
    }
}

fn decodificar(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    let mut r = String::with_capacity(s.len());
    let mut resto = s;
    while let Some(k) = resto.find('&') {
        r.push_str(&resto[..k]);
        resto = &resto[k..];
        let fin = resto[1..].find(|c: char| !(c.is_ascii_alphanumeric() || c == '#')).map(|j| j + 1);
        if let Some(j) = fin
            && resto.as_bytes()[j] == b';'
            && let Some(c) = entidad(&resto[1..j])
        {
            r.push(c);
            resto = &resto[j + 1..];
            continue;
        }
        r.push('&');
        resto = &resto[1..];
    }
    r.push_str(resto);
    r
}

fn entidad(n: &str) -> Option<char> {
    if let Some(num) = n.strip_prefix('#') {
        let v = match num.strip_prefix(['x', 'X']) {
            Some(h) => u32::from_str_radix(h, 16).ok()?,
            None => num.parse().ok()?,
        };
        return char::from_u32(v).filter(|&c| c != '\0');
    }
    const NOMBRES: &[(&str, char)] = &[
        ("amp", '&'), ("lt", '<'), ("gt", '>'), ("quot", '"'), ("apos", '\''), ("nbsp", '\u{a0}'), ("ensp", ' '),
        ("emsp", ' '), ("thinsp", ' '), ("shy", '\u{ad}'), ("ndash", '–'), ("mdash", '—'), ("hellip", '…'),
        ("lsquo", '‘'), ("rsquo", '’'), ("sbquo", '‚'), ("ldquo", '“'), ("rdquo", '”'), ("bdquo", '„'),
        ("laquo", '«'), ("raquo", '»'), ("bull", '•'), ("middot", '·'), ("copy", '©'), ("reg", '®'),
        ("trade", '™'), ("euro", '€'), ("pound", '£'), ("yen", '¥'), ("cent", '¢'), ("deg", '°'),
        ("plusmn", '±'), ("times", '×'), ("divide", '÷'), ("frac12", '½'), ("frac14", '¼'), ("frac34", '¾'),
        ("iexcl", '¡'), ("iquest", '¿'), ("para", '¶'), ("sect", '§'), ("ordf", 'ª'), ("ordm", 'º'),
        ("sup1", '¹'), ("sup2", '²'), ("sup3", '³'), ("micro", 'µ'), ("larr", '←'), ("rarr", '→'),
        ("uarr", '↑'), ("darr", '↓'), ("aacute", 'á'), ("Aacute", 'Á'), ("eacute", 'é'), ("Eacute", 'É'),
        ("iacute", 'í'), ("Iacute", 'Í'), ("oacute", 'ó'), ("Oacute", 'Ó'), ("uacute", 'ú'), ("Uacute", 'Ú'),
        ("ntilde", 'ñ'), ("Ntilde", 'Ñ'), ("uuml", 'ü'), ("Uuml", 'Ü'), ("ccedil", 'ç'), ("Ccedil", 'Ç'),
        ("agrave", 'à'), ("Agrave", 'À'), ("egrave", 'è'), ("Egrave", 'È'), ("igrave", 'ì'), ("ograve", 'ò'),
        ("ugrave", 'ù'), ("acirc", 'â'), ("ecirc", 'ê'), ("icirc", 'î'), ("ocirc", 'ô'), ("ucirc", 'û'),
        ("auml", 'ä'), ("euml", 'ë'), ("iuml", 'ï'), ("ouml", 'ö'), ("Auml", 'Ä'), ("Ouml", 'Ö'),
        ("atilde", 'ã'), ("otilde", 'õ'), ("aring", 'å'), ("oslash", 'ø'), ("aelig", 'æ'), ("szlig", 'ß'),
        ("yacute", 'ý'), ("yuml", 'ÿ'),
    ];
    NOMBRES.iter().find(|(k, _)| *k == n).map(|&(_, c)| c)
}

fn atributo(atributos: &str, nombre: &str) -> Option<String> {
    let b = atributos.as_bytes();
    let mut i = 0;
    while i < b.len() {
        while i < b.len() && (b[i].is_ascii_whitespace() || b[i] == b'/') {
            i += 1;
        }
        let ini = i;
        while i < b.len() && !b[i].is_ascii_whitespace() && !matches!(b[i], b'=' | b'/' | b'>') {
            i += 1;
        }
        let n = &atributos[ini..i];
        while i < b.len() && b[i].is_ascii_whitespace() {
            i += 1;
        }
        let mut valor = "";
        if i < b.len() && b[i] == b'=' {
            i += 1;
            while i < b.len() && b[i].is_ascii_whitespace() {
                i += 1;
            }
            if i < b.len() && (b[i] == b'"' || b[i] == b'\'') {
                let q = b[i];
                let v = i + 1;
                i = v + b[v..].iter().position(|&c| c == q).unwrap_or(b.len() - v);
                valor = &atributos[v..i];
                i = (i + 1).min(b.len());
            } else {
                let v = i;
                while i < b.len() && !b[i].is_ascii_whitespace() {
                    i += 1;
                }
                valor = &atributos[v..i];
            }
        }
        if !n.is_empty() && n.eq_ignore_ascii_case(nombre) {
            return Some(decodificar(valor));
        }
        if i == ini {
            i += 1;
        }
    }
    None
}

/// Lo que dice el atributo style de negrita, cursiva y tachado (Google Docs y muchas webs lo dicen así).
fn estilo(atributos: &str) -> (Option<bool>, Option<bool>, bool) {
    let mut r = (None, None, false);
    let Some(s) = atributo(atributos, "style") else { return r };
    for decl in s.to_ascii_lowercase().split(';') {
        let Some((k, v)) = decl.split_once(':') else { continue };
        let primero = v.split_whitespace().next().unwrap_or(""); // sin «!important»
        match k.trim() {
            "font-weight" => r.0 = Some(matches!(primero, "bold" | "bolder") || primero.parse::<u32>().is_ok_and(|n| n >= 600)),
            "font-style" => r.1 = Some(primero.starts_with("italic") || primero.starts_with("oblique")),
            "text-decoration" | "text-decoration-line" => r.2 |= v.contains("line-through"),
            _ => {}
        }
    }
    r
}

// --- Escribir el Markdown ---

/// El formato que ya tiene lo de alrededor: lo de dentro no lo vuelve a poner. Los `fuera_` son de un contenedor de
/// bloques (<b><p>…</p></b>, <div style="font-weight:700">): cada párrafo de dentro se envuelve con ellos.
#[derive(Clone, Copy, Default)]
struct Ctx {
    negrita: bool,
    cursiva: bool,
    tachado: bool,
    codigo: bool,
    enlace: bool,
    fuera_negrita: bool,
    fuera_cursiva: bool,
    fuera_tachado: bool,
}

/// Más celdas que estas no se pasan a tabla: el texto va como párrafos (una fila larguísima no se rellena).
const MAX_CELDAS: usize = 10_000;

/// El formato que un contenedor pone a sus párrafos, por su etiqueta o su estilo (que también lo puede quitar).
fn con_contenedor(e: &Elem, mut ctx: Ctx) -> Ctx {
    let (peso, inclinada, raya) = estilo(&e.atributos);
    let n = e.nombre.as_str();
    if let Some(si) = peso.or(matches!(n, "b" | "strong").then_some(true)) {
        ctx.fuera_negrita = si;
    }
    if let Some(si) = inclinada.or(matches!(n, "i" | "em").then_some(true)) {
        ctx.fuera_cursiva = si;
    }
    ctx.fuera_tachado |= raya || matches!(n, "s" | "strike" | "del");
    ctx
}

struct Conv {
    /// Ha salido algo de Markdown (si no, se pega el texto tal cual).
    formato: bool,
}

fn es_bloque(e: &Elem) -> bool {
    BLOQUES.contains(&e.nombre.as_str())
}

fn contiene_bloque(e: &Elem) -> bool {
    e.hijos.iter().any(|h| matches!(h, Nodo::Elem(x) if es_bloque(x) || contiene_bloque(x)))
}

impl Conv {
    /// Una serie de bloques; lo que va suelto entre ellos forma párrafos. `apretado`: un salto entre bloques, no dos
    /// (dentro de un elemento de lista o de una celda).
    fn bloques(&mut self, nodos: &[Nodo], ctx: Ctx, apretado: bool) -> String {
        // Cada parte, y si es un elemento de lista suelto (va pegado al anterior que también lo sea).
        let mut partes: Vec<(String, bool)> = Vec::new();
        let mut corrida: Vec<&Nodo> = Vec::new();
        for n in nodos {
            match n {
                Nodo::Elem(e) if es_bloque(e) => {
                    self.parrafo(&mut corrida, ctx, &mut partes);
                    if let Some(p) = self.bloque(e, ctx) {
                        partes.push(p);
                    }
                }
                // Un elemento de línea con bloques dentro (el <b> con que Google Docs envuelve todo, una tarjeta
                // de una web dentro de un enlace): cuenta como un contenedor más.
                Nodo::Elem(e) if contiene_bloque(e) => {
                    self.parrafo(&mut corrida, ctx, &mut partes);
                    let s = self.bloques(&e.hijos, con_contenedor(e, ctx), apretado);
                    if !s.is_empty() {
                        partes.push((s, false));
                    }
                }
                _ => corrida.push(n),
            }
        }
        self.parrafo(&mut corrida, ctx, &mut partes);
        let mut r = String::new();
        for (k, (t, suelto)) in partes.iter().enumerate() {
            if k > 0 {
                r.push_str(if apretado || (*suelto && partes[k - 1].1) { "\n" } else { "\n\n" });
            }
            r.push_str(t);
        }
        r
    }

    fn parrafo(&mut self, corrida: &mut Vec<&Nodo>, ctx: Ctx, partes: &mut Vec<(String, bool)>) {
        if corrida.is_empty() {
            return;
        }
        let (negrita, cursiva, tachado) = (ctx.fuera_negrita && !ctx.negrita, ctx.fuera_cursiva && !ctx.cursiva, ctx.fuera_tachado && !ctx.tachado);
        let dentro = Ctx { negrita: ctx.negrita || negrita, cursiva: ctx.cursiva || cursiva, tachado: ctx.tachado || tachado, ..ctx };
        let mut t = limpiar(&self.en_linea(corrida.drain(..), dentro));
        for (si, marca) in [(cursiva, "*"), (negrita, "**"), (tachado, "~~")] {
            if si {
                t = self.envolver(marca, t);
            }
        }
        if !t.is_empty() {
            partes.push((t.lines().map(escapar_inicio).collect::<Vec<_>>().join("\n"), false));
        }
    }

    fn bloque(&mut self, e: &Elem, ctx: Ctx) -> Option<(String, bool)> {
        let n = e.nombre.as_str();
        if let Some((nivel, numero)) = lista_word(e) {
            let t = una_linea(&limpiar(&self.en_linea(&e.hijos, ctx)));
            self.formato = true;
            let marca = numero.map_or("- ".to_string(), |k| format!("{k}. "));
            return Some((format!("{}{marca}{t}", "   ".repeat(nivel - 1)), true));
        }
        match n {
            "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                let t = una_linea(&limpiar(&self.en_linea(&e.hijos, Ctx { negrita: true, ..ctx })));
                if t.is_empty() {
                    return None;
                }
                self.formato = true;
                Some((format!("{} {t}", "#".repeat((n.as_bytes()[1] - b'0') as usize)), false))
            }
            "ul" | "ol" => self.lista(e, ctx).map(|s| (s, false)),
            "li" => {
                self.formato = true;
                Some((self.item("- ", e, ctx), true))
            }
            "blockquote" => {
                let dentro = self.bloques(&e.hijos, con_contenedor(e, ctx), false);
                if dentro.trim().is_empty() {
                    return None;
                }
                self.formato = true;
                let citado: Vec<String> = dentro.lines().map(|l| if l.is_empty() { ">".to_string() } else { format!("> {l}") }).collect();
                Some((citado.join("\n"), false))
            }
            "pre" => self.codigo(e).map(|s| (s, false)),
            "table" => self.tabla(e, ctx).map(|s| (s, false)),
            "hr" => {
                self.formato = true;
                Some(("---".into(), false))
            }
            _ => {
                let s = self.bloques(&e.hijos, con_contenedor(e, ctx), false);
                (!s.is_empty()).then_some((s, false))
            }
        }
    }

    fn item(&mut self, marca: &str, li: &Elem, ctx: Ctx) -> String {
        let dentro = self.bloques(&li.hijos, con_contenedor(li, ctx), true);
        if dentro.is_empty() {
            return marca.trim_end().to_string();
        }
        let sangria = " ".repeat(marca.len());
        let lineas: Vec<String> = dentro
            .lines()
            .enumerate()
            .map(|(k, l)| if k == 0 { format!("{marca}{l}") } else if l.is_empty() { String::new() } else { format!("{sangria}{l}") })
            .collect();
        lineas.join("\n")
    }

    fn lista(&mut self, e: &Elem, ctx: Ctx) -> Option<String> {
        let ordenada = e.nombre == "ol";
        // Un número de lista en Markdown tiene como mucho nueve cifras.
        const MAX: usize = 999_999_999;
        let mut n: usize = atributo(&e.atributos, "start").and_then(|s| s.trim().parse().ok()).filter(|&n| n <= MAX).unwrap_or(1);
        let mut lineas: Vec<String> = Vec::new();
        for h in &e.hijos {
            match h {
                Nodo::Elem(li) if li.nombre == "li" => {
                    let marca = if ordenada { format!("{n}. ") } else { "- ".to_string() };
                    n = (n + 1).min(MAX);
                    lineas.push(self.item(&marca, li, ctx));
                }
                // Una lista metida directamente en otra (no vale en HTML, pero se ve): cuelga del elemento anterior.
                Nodo::Elem(sub) if sub.nombre == "ul" || sub.nombre == "ol" => {
                    if let Some(s) = self.lista(sub, ctx) {
                        lineas.push(s.lines().map(|l| format!("  {l}")).collect::<Vec<_>>().join("\n"));
                    }
                }
                Nodo::Texto(t) if t.trim().is_empty() => {}
                otro => {
                    let s = self.bloques(std::slice::from_ref(otro), ctx, true);
                    if !s.is_empty() {
                        lineas.push(format!("- {s}"));
                    }
                }
            }
        }
        if lineas.is_empty() {
            return None;
        }
        self.formato = true;
        Some(lineas.join("\n"))
    }

    fn codigo(&mut self, e: &Elem) -> Option<String> {
        let mut t = String::new();
        texto_plano(&e.hijos, &mut t);
        let t = t.replace("\r\n", "\n").replace('\r', "\n").replace('\u{a0}', " ");
        let t = t.strip_prefix('\n').unwrap_or(&t).trim_end_matches('\n');
        if t.trim().is_empty() {
            return None;
        }
        let codigo_dentro = e.hijos.iter().find_map(|h| match h {
            Nodo::Elem(c) if c.nombre == "code" => Some(c.atributos.as_str()),
            _ => None,
        });
        let lengua = [Some(e.atributos.as_str()), codigo_dentro].into_iter().flatten().find_map(lengua).unwrap_or_default();
        // Más comillas que la tirada más larga de dentro: así ninguna línea del código cierra el bloque.
        let valla = "`".repeat((tirada(t, '`') + 1).max(3));
        self.formato = true;
        Some(format!("{valla}{lengua}\n{t}\n{valla}"))
    }

    fn tabla(&mut self, e: &Elem, ctx: Ctx) -> Option<String> {
        let mut filas: Vec<Vec<&Elem>> = Vec::new();
        recoger_filas(e, &mut filas);
        let columnas = filas.iter().map(Vec::len).max().unwrap_or(0);
        // El título de la tabla, como párrafo delante.
        let mut lineas: Vec<String> = e
            .hijos
            .iter()
            .filter_map(|h| match h {
                Nodo::Elem(x) if x.nombre == "caption" => Some(x),
                _ => None,
            })
            .map(|x| self.bloques(&x.hijos, ctx, false))
            .filter(|s| !s.is_empty())
            .map(|s| s + "\n")
            .collect();
        // Las de una fila o una columna, o con otra tabla dentro, son de maquetación (correos, webs): su contenido,
        // como bloques.
        if filas.len() < 2 || columnas < 2 || filas.len() * columnas > MAX_CELDAS || filas.iter().flatten().any(|c| contiene(c, "table")) {
            lineas.extend(filas.iter().flatten().map(|c| self.bloques(&c.hijos, ctx, false)).filter(|s| !s.is_empty()).map(|s| s + "\n"));
            return (!lineas.is_empty()).then(|| lineas.join("\n").trim_end().to_string());
        }
        for (k, f) in filas.iter().enumerate() {
            let mut celdas: Vec<String> = f.iter().map(|c| celda(&self.bloques(&c.hijos, ctx, true))).collect();
            celdas.resize(columnas, String::new());
            lineas.push(format!("| {} |", celdas.join(" | ")));
            if k == 0 {
                lineas.push(format!("| {} |", vec!["---"; columnas].join(" | ")));
            }
        }
        self.formato = true;
        Some(lineas.join("\n"))
    }

    fn en_linea<'a>(&mut self, nodos: impl IntoIterator<Item = &'a Nodo>, ctx: Ctx) -> String {
        let mut r = String::new();
        for n in nodos {
            match n {
                Nodo::Texto(t) => {
                    let t = espacios(t);
                    if ctx.codigo { r.push_str(&t) } else { r.push_str(&escapar(&t, ctx.enlace)) }
                }
                Nodo::MarcaWord(_) => {}
                Nodo::Elem(e) => juntar(&mut r, &self.elemento(e, ctx)),
            }
        }
        r
    }

    fn elemento(&mut self, e: &Elem, ctx: Ctx) -> String {
        let a = e.atributos.as_str();
        match e.nombre.as_str() {
            "br" => "\n".into(),
            "img" => {
                let alt = espacios(&atributo(a, "alt").unwrap_or_default());
                match atributo(a, "src") {
                    Some(src) if src.starts_with("http://") || src.starts_with("https://") => {
                        self.formato = true;
                        format!("![{}]({})", escapar(alt.trim(), true), url(&src))
                    }
                    _ => escapar(&alt, ctx.enlace),
                }
            }
            "input" if atributo(a, "type").is_some_and(|t| t.eq_ignore_ascii_case("checkbox")) => {
                self.formato = true;
                if atributo(a, "checked").is_some() { "[x] ".into() } else { "[ ] ".into() }
            }
            "a" => {
                let dentro = self.en_linea(&e.hijos, Ctx { enlace: true, ..ctx });
                let href = atributo(a, "href").unwrap_or_default();
                let href = href.trim();
                if ctx.enlace || href.is_empty() || href.starts_with('#') || href.to_ascii_lowercase().starts_with("javascript:") {
                    return dentro;
                }
                let t = dentro.trim_matches(' ');
                if t.is_empty() {
                    return dentro;
                }
                self.formato = true;
                let (ini, fin) = bordes(&dentro);
                // Un enlace o un código no cruzan saltos: un <br> de dentro es un espacio.
                let t = una_linea(t);
                let enlace = if t == href || href.strip_prefix("mailto:") == Some(t.as_str()) { url(href) } else { format!("[{t}]({})", url(href)) };
                format!("{ini}{enlace}{fin}")
            }
            "code" | "kbd" | "samp" | "tt" if !ctx.codigo => {
                let dentro = self.en_linea(&e.hijos, Ctx { codigo: true, ..ctx });
                let t = una_linea(dentro.trim_matches(' '));
                if t.is_empty() {
                    return dentro;
                }
                self.formato = true;
                let (ini, fin) = bordes(&dentro);
                let comilla = "`".repeat(tirada(&t, '`') + 1);
                let hueco = if t.starts_with('`') || t.ends_with('`') { " " } else { "" };
                format!("{ini}{comilla}{hueco}{t}{hueco}{comilla}{fin}")
            }
            "math" => {
                let mut tex = String::new();
                texto_plano(&e.hijos, &mut tex);
                self.formato = true;
                if atributo(a, "display").is_some_and(|d| d.eq_ignore_ascii_case("block")) {
                    format!("\n$${}$$\n", tex.trim())
                } else {
                    format!("${}$", una_linea(&tex))
                }
            }
            // Un bloque dentro de algo de una línea (un título con un div dentro): su texto, en su línea.
            _ if es_bloque(e) => format!("\n{}\n", self.en_linea(&e.hijos, ctx).trim_matches('\n')),
            n => {
                let (peso, inclinada, raya) = estilo(a);
                let negrita = peso.unwrap_or(matches!(n, "b" | "strong")) && !ctx.negrita;
                let cursiva = inclinada.unwrap_or(matches!(n, "i" | "em")) && !ctx.cursiva;
                let tachado = (raya || matches!(n, "s" | "strike" | "del")) && !ctx.tachado;
                let dentro = self.en_linea(
                    &e.hijos,
                    Ctx { negrita: ctx.negrita || negrita, cursiva: ctx.cursiva || cursiva, tachado: ctx.tachado || tachado, ..ctx },
                );
                let mut s = dentro;
                for (si, marca) in [(cursiva, "*"), (negrita, "**"), (tachado, "~~")] {
                    if si && !ctx.codigo {
                        s = self.envolver(marca, s);
                    }
                }
                s
            }
        }
    }

    /// Pone la marca a cada lado, línea a línea (una marca no cruza saltos) y con los espacios del borde fuera:
    /// «**hola **» no sería negrita.
    fn envolver(&mut self, marca: &str, s: String) -> String {
        let lineas: Vec<String> = s
            .split('\n')
            .map(|l| {
                let t = l.trim_matches(' ');
                if t.is_empty() {
                    return l.to_string();
                }
                self.formato = true;
                let (ini, fin) = bordes(l);
                format!("{ini}{marca}{t}{marca}{fin}")
            })
            .collect();
        lineas.join("\n")
    }
}

/// Los espacios del principio y del final.
fn bordes(s: &str) -> (&str, &str) {
    let ini = &s[..s.len() - s.trim_start_matches(' ').len()];
    let fin = &s[s.trim_end_matches(' ').len()..];
    (ini, fin)
}

fn lista_word(e: &Elem) -> Option<(usize, Option<usize>)> {
    if e.nombre != "p" {
        return None;
    }
    let s = atributo(&e.atributos, "style")?.to_ascii_lowercase();
    let i = s.find("mso-list:l")?;
    let marca = marca_word(&e.hijos)?;
    let nivel = s[i..].find("level").map_or(1, |k| {
        let cifras: String = s[i + k + 5..].chars().take_while(char::is_ascii_digit).collect();
        cifras.parse().unwrap_or(1)
    });
    let m: String = marca.chars().filter(|c| !c.is_whitespace()).collect();
    let numerada = m.len() > 1 && m.ends_with(['.', ')']) && m.chars().next().is_some_and(char::is_alphanumeric);
    let numero = numerada.then(|| m.trim_end_matches(['.', ')']).parse().unwrap_or(1));
    Some((nivel.clamp(1, 9), numero))
}

fn marca_word(nodos: &[Nodo]) -> Option<&str> {
    nodos.iter().find_map(|n| match n {
        Nodo::MarcaWord(m) => Some(m.as_str()),
        Nodo::Elem(e) => marca_word(&e.hijos),
        Nodo::Texto(_) => None,
    })
}

fn texto_plano(nodos: &[Nodo], r: &mut String) {
    for n in nodos {
        match n {
            Nodo::Texto(t) => r.push_str(t),
            Nodo::Elem(e) if e.nombre == "br" => r.push('\n'),
            Nodo::Elem(e) => {
                texto_plano(&e.hijos, r);
                if es_bloque(e) && !r.ends_with('\n') {
                    r.push('\n');
                }
            }
            Nodo::MarcaWord(_) => {}
        }
    }
}

/// La lengua de un bloque de código, de su clase («language-rust», «lang-js»).
fn lengua(atributos: &str) -> Option<String> {
    let clase = atributo(atributos, "class")?;
    clase.split_whitespace().find_map(|c| {
        let l = c.strip_prefix("language-").or_else(|| c.strip_prefix("lang-"))?;
        let l: String = l.chars().take_while(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '#' | '-')).collect();
        (!l.is_empty()).then_some(l)
    })
}

fn recoger_filas<'a>(e: &'a Elem, filas: &mut Vec<Vec<&'a Elem>>) {
    for h in &e.hijos {
        if let Nodo::Elem(x) = h {
            match x.nombre.as_str() {
                "tr" => filas.push(
                    x.hijos.iter().filter_map(|c| match c {
                        Nodo::Elem(c) if c.nombre == "td" || c.nombre == "th" => Some(c),
                        _ => None,
                    }).collect(),
                ),
                "thead" | "tbody" | "tfoot" => recoger_filas(x, filas),
                _ => {}
            }
        }
    }
}

fn contiene(e: &Elem, nombre: &str) -> bool {
    e.hijos.iter().any(|h| matches!(h, Nodo::Elem(x) if x.nombre == nombre || contiene(x, nombre)))
}

fn celda(s: &str) -> String {
    let mut r = String::new();
    for ch in una_linea(s).chars() {
        // Con un número impar de barras delante ya va escapado: lo hace `escapar_inicio` al principio de un párrafo.
        if ch == '|' && (r.len() - r.trim_end_matches('\\').len()).is_multiple_of(2) {
            r.push('\\');
        }
        r.push(ch);
    }
    r
}

/// Pone `x` detrás de `r`. Si los dos acaban y empiezan con la misma marca (`**Impor**` y `**tante**`: Word y Outlook
/// parten así las tiradas), las de en medio sobran: pegadas no cerrarían ni abrirían nada.
fn juntar(r: &mut String, x: &str) {
    if let Some(c) = x.chars().next().filter(|c| matches!(c, '*' | '~' | '`')) {
        let n = x.len() - x.trim_start_matches(c).len();
        // Sin recorrer la tirada entera de `r`: dentro de un código puede crecer sin fin (y juntar sería cuadrático).
        let tirada = r.bytes().rev().take(n + 1).take_while(|&b| b == c as u8).count();
        let sin = &r[..r.len() - tirada];
        if tirada == n && x.len() > n && !sin.is_empty() && !sin.ends_with('\\') {
            r.truncate(sin.len());
            r.push_str(&x[n..]);
            return;
        }
    }
    r.push_str(x);
}

fn url(u: &str) -> String {
    u.replace(' ', "%20").replace('(', "%28").replace(')', "%29")
}

/// Como lo enseña un navegador: cada tramo de espacios y saltos, un espacio.
fn espacios(t: &str) -> String {
    let mut r = String::with_capacity(t.len());
    let mut hueco = false;
    for c in t.chars().filter(|&c| c != '\u{ad}') {
        if c.is_whitespace() {
            hueco = true;
        } else {
            if hueco {
                r.push(' ');
                hueco = false;
            }
            r.push(c);
        }
    }
    if hueco {
        r.push(' ');
    }
    r
}

/// Las líneas sin espacios de más ni en los bordes, y como mucho una vacía seguida.
fn limpiar(s: &str) -> String {
    let mut r: Vec<String> = Vec::new();
    for l in s.split('\n') {
        let l = l.split(' ').filter(|p| !p.is_empty()).collect::<Vec<_>>().join(" ");
        if l.is_empty() && r.last().is_none_or(String::is_empty) {
            continue;
        }
        r.push(l);
    }
    while r.last().is_some_and(String::is_empty) {
        r.pop();
    }
    r.join("\n")
}

/// La tirada más larga de un carácter seguido.
fn tirada(s: &str, ch: char) -> usize {
    s.split(|c| c != ch).map(str::len).max().unwrap_or(0)
}

fn una_linea(s: &str) -> String {
    s.split('\n').map(str::trim).filter(|l| !l.is_empty()).collect::<Vec<_>>().join(" ")
}

/// Lo que en un texto se tomaría por una marca de Markdown, con su barra delante.
fn escapar(t: &str, en_enlace: bool) -> String {
    let c: Vec<char> = t.chars().collect();
    let alfa = |i: Option<&char>| i.is_some_and(|c| c.is_alphanumeric());
    let mut r = String::with_capacity(t.len());
    for (k, &ch) in c.iter().enumerate() {
        let antes = k.checked_sub(1).and_then(|i| c.get(i));
        let escapa = match ch {
            '\\' | '*' | '`' => true,
            // Entre letras no es cursiva (nombre_de_archivo).
            '_' => !(alfa(antes) && alfa(c.get(k + 1))),
            '~' => c.get(k + 1) == Some(&'~') || antes == Some(&'~'),
            // [a](b) sería un enlace; <i> o &lt; serían HTML en otros visores.
            '[' => true,
            ']' => en_enlace,
            '<' => c.get(k + 1).is_some_and(|n| n.is_ascii_alphabetic() || matches!(n, '/' | '!' | '?')),
            '&' => c.get(k + 1).is_some_and(|n| n.is_ascii_alphanumeric() || *n == '#'),
            _ => false,
        };
        if escapa {
            r.push('\\');
        }
        r.push(ch);
    }
    r
}

/// Un párrafo cuya línea empieza como un título, una cita, una lista, una tabla o una raya: con su barra delante.
fn escapar_inicio(l: &str) -> String {
    let b = l.as_bytes();
    let cifras = b.iter().take_while(|c| c.is_ascii_digit()).count();
    let raya = |c: u8| l.len() >= 3 && b.iter().all(|&x| x == c);
    match b.first() {
        Some(b'#' | b'>' | b'|') => format!("\\{l}"),
        Some(b'-' | b'+') if b.get(1) == Some(&b' ') || raya(b[0]) => format!("\\{l}"),
        Some(b'=') if raya(b'=') => format!("\\{l}"),
        Some(_) if cifras > 0 && matches!(b.get(cifras), Some(b'.' | b')')) && matches!(b.get(cifras + 1), None | Some(b' ')) => {
            format!("{}\\{}", &l[..cifras], &l[cifras..])
        }
        _ => l.to_string(),
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    fn md(h: &str) -> String {
        a_markdown(h).unwrap_or_else(|| "(sin formato)".into())
    }

    #[test]
    fn navegador() {
        let h = "<h2>Título</h2><p>Texto con <strong>negrita</strong>, <em>cursiva</em> y <a href=\"https://x.com/a b\">enlace</a>.</p>\
                 <ul><li>uno</li><li>dos<ul><li>dos.uno</li></ul></li></ul><ol start=\"3\"><li>tres</li></ol>\
                 <blockquote><p>cita</p></blockquote><pre><code class=\"language-rust\">fn main() {\n    x\n}</code></pre>\
                 <table><tr><th>a</th><th>b</th></tr><tr><td>1</td><td>2|3</td></tr></table><hr>";
        let esperado = "## Título\n\nTexto con **negrita**, *cursiva* y [enlace](https://x.com/a%20b).\n\n- uno\n- dos\n  - dos.uno\n\n\
                        3. tres\n\n> cita\n\n```rust\nfn main() {\n    x\n}\n```\n\n| a | b |\n| --- | --- |\n| 1 | 2\\|3 |\n\n---";
        assert_eq!(md(h), esperado);
    }

    #[test]
    fn tramos_pegados_y_celdas() {
        assert_eq!(md("<p><b>Impor</b><b>tante</b> y <b>Nota</b><b>:</b> <i>a</i><b>b</b></p>"), "**Importante** y **Nota:** *a***b**");
        assert_eq!(md("<table><tr><th>a</th><th>b</th></tr><tr><td>| Opción</td><td>2</td></tr></table>"), "| a | b |\n| --- | --- |\n| \\| Opción | 2 |");
    }

    #[test]
    fn word() {
        let h = "<p class=MsoNormal><b><span style='font-size:14.0pt'>Informe<o:p></o:p></span></b></p>\r\n\
                 <p class=MsoListParagraphCxSpFirst style='text-indent:-18.0pt;mso-list:l0 level1 lfo1'><![if !supportLists]><span\r\n\
                 style='font-family:Symbol'><span style='mso-list:Ignore'>·<span style='font:7.0pt \"Times New Roman\"'>&nbsp;&nbsp;&nbsp;\r\n\
                 </span></span></span><![endif]>Primero<o:p></o:p></p>\r\n\
                 <p class=MsoListParagraphCxSpLast style='text-indent:-18.0pt;mso-list:l0 level2 lfo1'><![if !supportLists]><span\r\n\
                 style='font-family:\"Courier New\"'><span style='mso-list:Ignore'>o<span style='font:7.0pt \"Times New Roman\"'>&nbsp;&nbsp;\r\n\
                 </span></span></span><![endif]>Segundo con <i>cursiva</i><o:p></o:p></p>\r\n\
                 <p class=MsoListParagraph style='mso-list:l1 level1 lfo2'><![if !supportLists]><span>2.<span>&nbsp; </span></span><![endif]>Numerado</p>\r\n\
                 <p class=MsoNormal><o:p>&nbsp;</o:p></p>\r\n<p class=MsoNormal>Fin&nbsp;del texto.</p>";
        assert_eq!(md(h), "**Informe**\n\n- Primero\n   - Segundo con *cursiva*\n2. Numerado\n\nFin del texto.");
    }

    #[test]
    fn google_docs() {
        let h = "<meta charset=\"utf-8\"><b style=\"font-weight:normal;\" id=\"docs-internal-guid-1\"><h1 dir=\"ltr\"><span \
                 style=\"font-size:20pt;font-weight:400;\">Plan</span></h1><p dir=\"ltr\"><span style=\"font-weight:700;\">Negrita</span>\
                 <span style=\"font-weight:400;\"> y </span><span style=\"font-style:italic;font-weight:400;\">cursiva</span></p></b>";
        assert_eq!(md(h), "# Plan\n\n**Negrita** y *cursiva*");
    }

    #[test]
    fn editor_de_codigo_sin_formato() {
        let h = "<meta charset='utf-8'><div style=\"color: #cccccc;white-space: pre;\"><div><span style=\"color: #569cd6;\">fn</span>\
                 <span> </span><span>main</span>() {}</div></div>";
        assert!(a_markdown(h).is_none());
        assert!(a_markdown("<p>solo texto</p>").is_none());
        // Varias líneas, en su bloque con la lengua de VS Code; una sola, tal cual.
        let lengua = lengua_de_vscode("{\"version\":1,\"isFromEmptySelection\":false,\"mode\":\"rust\"}");
        assert_eq!(pegado(Some(h), Some("fn main() {\r\n    x\r\n}\r\n"), &lengua), Some(("```rust\nfn main() {\n    x\n}\n```".into(), true)));
        assert_eq!(pegado(Some(h), Some("fn main() {}"), &lengua), None);
        assert_eq!(pegado(Some(h), Some("a\n```\nb"), ""), Some(("````\na\n```\nb\n````".into(), true)));
        assert_eq!(lengua_de_vscode("{\"mode\": \"typescriptreact\"}"), "tsx");
        assert_eq!(lengua_de_vscode("{\"mode\":\"plaintext\"}"), "");
    }

    #[test]
    fn formulas_de_katex() {
        let katex = |tex: &str, bloque: bool| {
            format!(
                "<span class=\"katex\"><span class=\"katex-mathml\"><math xmlns=\"http://www.w3.org/1998/Math/MathML\"{}><semantics><mrow><msup><mi>x</mi><mn>2</mn></msup></mrow>\
                 <annotation encoding=\"application/x-tex\">{tex}</annotation></semantics></math></span><span class=\"katex-html\" aria-hidden=\"true\">\
                 <span class=\"base\"><span class=\"mord\">x<span class=\"msupsub\">2</span></span></span></span></span>",
                if bloque { " display=\"block\"" } else { "" }
            )
        };
        assert_eq!(md(&format!("<p>Vale {} si x &gt; 0.</p>", katex("x^2", false))), "Vale $x^2$ si x > 0.");
        assert_eq!(md(&format!("<p>Antes</p><span class=\"katex-display\">{}</span><p>Después</p>", katex("\\frac{a}{b}", true))), "Antes\n\n$$\\frac{a}{b}$$\n\nDespués");
        // Sin la anotación, como antes: la fórmula de MathML no sale y lo dibujado, sí.
        assert_eq!(md("<p><b>y</b> <span aria-hidden=\"true\">x2</span></p>"), "**y** x2");
    }

    #[test]
    fn tabuladores_en_tabla() {
        assert_eq!(pegado(None, Some("Nombre\tEdad\r\nAna\t30\r\nLuis|x\t4\r\n"), ""), Some(("| Nombre | Edad |\n| --- | --- |\n| Ana | 30 |\n| Luis\\|x | 4 |".into(), true)));
        assert_eq!(pegado(None, Some("if x {\n\ty\n}"), ""), None, "código sangrado");
        assert_eq!(pegado(None, Some("a\tb\nc"), ""), None, "distintos tabuladores");
        assert_eq!(pegado(None, Some("a\tb"), ""), None, "una línea");
    }

    #[test]
    fn espacios_y_escapes() {
        assert_eq!(md("<b>hola </b>mundo"), "**hola** mundo");
        assert_eq!(md("<p>a<br>b <i> </i>c</p><p><b>f</b></p>"), "a\nb c\n\n**f**");
        assert_eq!(
            md("<p>a*b 1_2 _x_ \\ ~~no~~</p><p>#Título falso</p><p>1. no es lista</p><p>- tampoco</p><p><b>f</b></p>"),
            "a\\*b 1_2 \\_x\\_ \\\\ \\~\\~no\\~\\~\n\n\\#Título falso\n\n1\\. no es lista\n\n\\- tampoco\n\n**f**"
        );
        assert_eq!(md("<b><b>doble</b></b> <i>a<br>b</i>"), "**doble** *a*\n*b*");
        assert_eq!(md("<code>a`b</code> y <a href=\"https://e.es\">https://e.es</a>"), "``a`b`` y https://e.es");
        assert_eq!(md("caf&eacute; &amp; t&#233; &#x41; &bogus; &"), "(sin formato)");
        assert_eq!(md("<b>caf&eacute; &amp; t&#233; &#x41; &bogus; &</b>"), "**café & té A \\&bogus; &**");
    }

    #[test]
    fn arreglos_de_la_revision() {
        assert_eq!(md("<div style=\"font-weight:700\">A</div><b>B</b>"), "**A**\n\n**B**", "negrita de un contenedor");
        assert_eq!(md("<b><p>A</p><p>C</p></b>"), "**A**\n\n**C**");
        assert_eq!(md("<b style=\"font-weight:normal\"><p><b>x</b> y</p></b>"), "**x** y", "el envoltorio de Google Docs no pone negrita");
        assert_eq!(md("<b style=\"font-weight: bold !important\">x</b>"), "**x**");
        assert_eq!(
            md("<table><caption>Total</caption><tr><td>a</td><td>b</td></tr><tr><td>1</td><td>2</td></tr></table>"),
            "Total\n\n| a | b |\n| --- | --- |\n| 1 | 2 |"
        );
        assert_eq!(md("<code>a``b</code>"), "```a``b```", "una comilla más que las de dentro");
        assert_eq!(md("<pre>a\rb  \n```\n</pre>"), "````\na\nb  \n```\n````", "saltos CR y una valla que no se cierra dentro");
        assert_eq!(md("<b>[x](u) &lt;i&gt;y&lt;/i&gt;</b>"), "**\\[x](u) \\<i>y\\</i>**", "texto que parecería Markdown o HTML");
        assert_eq!(md("<p>| Opción A</p><p><b>b</b></p>"), "\\| Opción A\n\n**b**", "un párrafo que parecería tabla");
        assert_eq!(md("<ol start=\"18446744073709551615\"><li>x</li></ol>"), "1. x");
        assert_eq!(md("<ol start=\"999999999\"><li>x</li><li>y</li></ol>"), "999999999. x\n999999999. y");
        assert_eq!(md("<p><![if !supportLists]>· Hola</p><p><b>sigue</b></p>"), "· Hola\n\n**sigue**", "una viñeta de Word cortada");
        assert_eq!(md("<a href=\"https://a.es\">a<br>b</a>"), "[a b](https://a.es)");
        assert!(a_markdown("<div style=\"white-space: pre;\"><div><i>// nota</i></div><div>    x = 1</div></div>").is_none(), "editor de código con cursiva");
        let enorme = format!("<table><tr>{}</tr>{}</table>", "<td>x</td>".repeat(2000), "<tr><td>y</td><td>z</td></tr>".repeat(100));
        let t = std::time::Instant::now();
        assert!(a_markdown(&enorme).is_none_or(|m| !m.contains("| ---")) && t.elapsed().as_millis() < 500, "tabla enorme como párrafos");
    }

    #[test]
    fn listas_tablas_y_maqueta() {
        assert_eq!(md("<ul><li><input type=\"checkbox\" checked> hecho</li><li><input type=checkbox> no</li></ul>"), "- [x] hecho\n- [ ] no");
        assert_eq!(md("<li>suelto</li><li>otro</li>"), "- suelto\n- otro");
        assert_eq!(md("<table><tr><td><p>Hola <b>Ana</b></p><p>Texto</p></td></tr></table>"), "Hola **Ana**\n\nTexto");
        assert_eq!(md("<table><tr><td>a<td>b<tr><td>1</table>"), "| a | b |\n| --- | --- |\n| 1 |  |");
        assert_eq!(md("<p>uno<p>dos <b>tres</b>"), "uno\n\ndos **tres**");
        assert_eq!(md("<a href=\"https://w.es\"><div><h3>Tarjeta</h3></div></a>"), "### Tarjeta");
        assert_eq!(md("<img src=\"https://i.es/a.png\" alt=\"foto [1]\"><img src=\"data:image/png;base64,xx\" alt=\"no\">"), "![foto \\[1\\]](https://i.es/a.png)no");
    }

    #[test]
    fn fragmento_del_portapapeles() {
        let html = "<html><body><!--StartFragment--><b>x</b><!--EndFragment--></body></html>";
        let ini = html.find("<b>").unwrap();
        let fin = html.find("<!--EndFragment").unwrap();
        let cab = |a: usize, b: usize| format!("Version:0.9\r\nStartHTML:-1\r\nEndHTML:-1\r\nStartFragment:{a:010}\r\nEndFragment:{b:010}\r\n");
        let n = cab(0, 0).len();
        let datos = format!("{}{html}\0basura", cab(n + ini, n + fin));
        assert_eq!(fragmento(datos.as_bytes()), "<b>x</b>");
        assert_eq!(fragmento(b"Version:1.0\r\n<i>y</i>"), "<i>y</i>");
    }

    #[test]
    fn html_raro_no_cuelga() {
        let piezas = [
            "<b>", "</b>", "<p>", "</p>", "<ul>", "<li>", "</ul>", "<ol>", "<table>", "<tr>", "<td>", "</td>", "</table>", "<pre>",
            "</pre>", "<a href='x'>", "<a href=\"https://a.es\">", "</a>", "texto", "&amp;", "&#x41;", "&#xFFFFFFFF;", "&", "<", ">",
            "<!--", "-->", "<![if !supportLists]>", "<![endif]>", "\n", " ", "<br>", "<h1>", "</h1>", "<blockquote>", "á",
            "<img src='http://x/y.png' alt='a'>", "<code>", "</code>", "<i>", "</i>", "<s>", "<div>", "</div>", "*_`~[]#",
            "<span style='font-weight:700'>", "</span>", "<p style='mso-list:l0 level2 lfo1'>", "<style>", "</style>", "<x y='>'>",
            "<input type=checkbox checked>", "<th>", "<hr>", "<?xml?>", "<!DOCTYPE html>", "<o:p>", "</o:p>", "1. ", "- ",
        ];
        let mut semilla: u64 = 7;
        let mut azar = |n: usize| {
            semilla = semilla.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            ((semilla >> 33) % n as u64) as usize
        };
        for _ in 0..3000 {
            let h: String = (0..azar(40)).map(|_| piezas[azar(piezas.len())]).collect();
            let _ = a_markdown(&h);
            let _ = fragmento(h.as_bytes());
        }
        let _ = a_markdown(&"<div><b>".repeat(50_000));
        assert_eq!(md(&("<div>".repeat(50) + "<b>x</b>")), "**x**");
    }
}
