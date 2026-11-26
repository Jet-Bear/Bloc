// Deshacer de varios niveles de los Markdown en vivo (el texto plano usa el de RichEdit, que no ve el formato).
// Cada cambio se guarda como la diferencia con el texto anterior; lo que se teclea seguido se agrupa por palabras.

#[derive(Clone)]
pub struct Cambio {
    pub pos: usize,
    pub quitado: Vec<u16>,
    pub puesto: Vec<u16>,
    /// La selección de antes y de después, si el cambio es de una acción de Bloc (botón, Tab, Intro, Retroceso): no
    /// coincide con lo cambiado. Sin ella, deshacer selecciona lo que vuelve y rehacer lo que se pone.
    pub antes: Option<(usize, usize)>,
    pub despues: Option<(usize, usize)>,
}

impl Cambio {
    fn peso(&self) -> usize {
        self.quitado.len() + self.puesto.len()
    }
}

// Tope de lo guardado, en pasos y en caracteres: unos cuantos «Reemplazar todo» sobre un archivo grande no deben
// llenar la memoria.
const MAX_PASOS: usize = 2000;
const MAX_CARACTERES: usize = 10_000_000;

#[derive(Default)]
pub struct Historial {
    atras: Vec<Cambio>,
    adelante: Vec<Cambio>,
    agrupar: bool,
    carga: usize,
    /// Cuántos pasos se han anotado en total (no baja al deshacer ni al quitar los más viejos).
    anotados: u64,
}

/// Lo que no ha cambiado por delante y por detrás (prefijo, sufijo), o None si los dos textos son iguales.
pub fn diferencia(antes: &[u16], despues: &[u16]) -> Option<(usize, usize)> {
    let max = antes.len().min(despues.len());
    // Los cortes nunca caen entre las dos mitades de un carácter (un emoji, por ejemplo): deshacer lo partiría.
    let segunda_mitad = |c: Option<&u16>| c.is_some_and(|&c| (0xDC00..0xE000).contains(&c));
    let mut p = antes.iter().zip(despues).take_while(|(x, y)| x == y).count();
    while p > 0 && (segunda_mitad(antes.get(p)) || segunda_mitad(despues.get(p))) {
        p -= 1;
    }
    let mut s = antes[p..].iter().rev().zip(despues[p..].iter().rev()).take(max - p).take_while(|(x, y)| x == y).count();
    while s > 0 && segunda_mitad(antes.get(antes.len() - s)) {
        s -= 1;
    }
    if p + s == antes.len() && p + s == despues.len() { None } else { Some((p, s)) }
}

impl Historial {
    pub fn limpiar(&mut self) {
        *self = Historial::default();
    }

    /// Anota el cambio de «antes» a «despues», que difieren en todo menos en p caracteres delante y s detrás.
    pub fn anotar(&mut self, antes: &[u16], despues: &[u16], p: usize, s: usize) {
        let c = Cambio { pos: p, quitado: antes[p..antes.len() - s].to_vec(), puesto: despues[p..despues.len() - s].to_vec(), antes: None, despues: None };
        self.carga -= self.adelante.iter().map(Cambio::peso).sum::<usize>();
        self.adelante.clear();
        let peso = c.peso();
        let corta = |c: u16| c == b' ' as u16 || c == 13;
        let unido = match self.atras.last_mut() {
            Some(u) if self.agrupar && u.antes.is_none() && u.despues.is_none() => {
                if u.quitado.is_empty() && c.quitado.is_empty() && c.puesto.len() == 1 && c.pos == u.pos + u.puesto.len()
                    && !(corta(c.puesto[0]) && !u.puesto.last().is_some_and(|&l| corta(l)))
                {
                    u.puesto.push(c.puesto[0]);
                    true
                } else if u.puesto.is_empty() && c.puesto.is_empty() && c.quitado.len() == 1 && c.pos + 1 == u.pos {
                    u.pos = c.pos;
                    u.quitado.insert(0, c.quitado[0]);
                    true
                } else if u.puesto.is_empty() && c.puesto.is_empty() && c.quitado.len() == 1 && c.pos == u.pos {
                    u.quitado.push(c.quitado[0]);
                    true
                } else {
                    false
                }
            }
            _ => false,
        };
        if !unido {
            self.atras.push(c);
            self.anotados += 1;
        }
        self.carga += peso;
        while self.atras.len() > 1 && (self.atras.len() > MAX_PASOS || self.carga > MAX_CARACTERES) {
            let x = self.atras.remove(0);
            self.carga -= x.peso();
        }
        self.agrupar = peso == 1;
    }

    /// Lo siguiente que se anote no se junta con lo de antes.
    pub fn cortar(&mut self) {
        self.agrupar = false;
    }

    pub fn anotados(&self) -> u64 {
        self.anotados
    }

    /// Una acción de Bloc ha anotado los pasos desde que había `desde` anotados: deshacerla vuelve a la selección
    /// `antes` y rehacerla, a `despues`.
    pub fn selecciones(&mut self, desde: u64, antes: (usize, usize), despues: (usize, usize)) {
        let nuevos = self.anotados.saturating_sub(desde) as usize;
        if nuevos == 0 || nuevos > self.atras.len() {
            return;
        }
        let k = self.atras.len() - nuevos;
        self.atras[k].antes = Some(antes);
        if let Some(u) = self.atras.last_mut() {
            u.despues = Some(despues);
        }
    }

    pub fn puede_deshacer(&self) -> bool {
        !self.atras.is_empty()
    }

    pub fn puede_rehacer(&self) -> bool {
        !self.adelante.is_empty()
    }

    /// El cambio a revertir: hay que poner `quitado` en lugar de [pos, pos + puesto.len()).
    pub fn deshacer(&mut self) -> Option<Cambio> {
        let c = self.atras.pop()?;
        self.adelante.push(c.clone());
        self.agrupar = false;
        Some(c)
    }

    /// El cambio a repetir: hay que poner `puesto` en lugar de [pos, pos + quitado.len()).
    pub fn rehacer(&mut self) -> Option<Cambio> {
        let c = self.adelante.pop()?;
        self.atras.push(c.clone());
        self.agrupar = false;
        Some(c)
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    fn u(s: &str) -> Vec<u16> {
        s.encode_utf16().collect()
    }

    #[test]
    fn no_parte_un_emoji() {
        // 😀 y 😁 comparten la primera mitad; el cambio tiene que llevar las dos.
        assert_eq!(diferencia(&u("a😀b"), &u("a😁b")), Some((1, 1)));
        assert_eq!(diferencia(&u("😀"), &u("😀x")), Some((2, 0)));
    }

    fn anotar(h: &mut Historial, antes: &[u16], despues: &[u16]) -> bool {
        match diferencia(antes, despues) {
            Some((p, s)) => {
                h.anotar(antes, despues, p, s);
                true
            }
            None => false,
        }
    }

    fn revertir(h: &mut Historial, s: &[u16]) -> Vec<u16> {
        match h.deshacer() {
            Some(c) => [&s[..c.pos], &c.quitado[..], &s[c.pos + c.puesto.len()..]].concat(),
            None => s.to_vec(),
        }
    }

    fn repetir(h: &mut Historial, s: &[u16]) -> Vec<u16> {
        match h.rehacer() {
            Some(c) => [&s[..c.pos], &c.puesto[..], &s[c.pos + c.quitado.len()..]].concat(),
            None => s.to_vec(),
        }
    }

    #[test]
    fn agrupa_por_palabras() {
        let mut h = Historial::default();
        let mut s = u("");
        for paso in ["h", "ho", "hol", "hola", "hola ", "hola m", "hola mu"] {
            anotar(&mut h, &s, &u(paso));
            s = u(paso);
        }
        s = revertir(&mut h, &s);
        assert_eq!(s, u("hola"), "deshacer quita la última palabra");
        s = revertir(&mut h, &s);
        assert_eq!(s, u(""), "deshacer otra vez quita la primera");
        s = repetir(&mut h, &s);
        s = repetir(&mut h, &s);
        assert_eq!(s, u("hola mu"), "rehacer las dos");
        anotar(&mut h, &s, &u("hola"));
        s = u("hola");
        anotar(&mut h, &s, &u("hXola"));
        s = u("hXola");
        s = revertir(&mut h, &s);
        assert_eq!(s, u("hola"), "deshacer un cambio en medio");
        s = revertir(&mut h, &s);
        assert_eq!(s, u("hola mu"), "deshacer un borrado");
        for paso in ["hola m", "hola ", "hola"] {
            anotar(&mut h, &s, &u(paso));
            s = u(paso);
        }
        s = revertir(&mut h, &s);
        assert_eq!(s, u("hola mu"), "los retrocesos seguidos se deshacen de una vez");
        assert!(!anotar(&mut h, &s, &s.clone()), "sin cambio no se anota");
    }

    #[test]
    fn un_salto_empieza_otro_paso() {
        let mut h = Historial::default();
        let mut s = u("");
        for paso in ["u", "un", "un\r", "un\rd", "un\rdo"] {
            anotar(&mut h, &s, &u(paso));
            s = u(paso);
        }
        s = revertir(&mut h, &s);
        assert_eq!(s, u("un"), "deshacer quita la línea nueva con su salto");
        s = revertir(&mut h, &s);
        assert_eq!(s, u(""), "y otra vez, la de antes");
    }

    #[test]
    fn tope_de_memoria() {
        let mut h = Historial::default();
        let mil = vec![b'a' as u16; 1_000_000];
        let otro = vec![b'b' as u16; 1_000_000];
        for k in 0..20 {
            let (a, b) = if k % 2 == 0 { (&mil, &otro) } else { (&otro, &mil) };
            anotar(&mut h, a, b);
        }
        let mut pasos = 0;
        while h.deshacer().is_some() {
            pasos += 1;
        }
        assert!((1..=5).contains(&pasos), "no guarda más de 10 millones de caracteres ({pasos} pasos)");
    }

    // Generador sencillo y repetible: las pruebas no necesitan nada de fuera.
    struct Azar(u64);
    impl Azar {
        fn n(&mut self, max: usize) -> usize {
            self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            ((self.0 >> 33) % max.max(1) as u64) as usize
        }
        fn texto(&mut self, n: usize) -> Vec<u16> {
            let letras = u("abc dé\rxyz ñ*");
            (0..n).map(|_| letras[self.n(letras.len())]).collect()
        }
    }

    #[test]
    fn estres() {
        let mut r = Azar(7);
        for _ in 0..500 {
            let mut h = Historial::default();
            let n = r.n(40);
            let original = r.texto(n);
            let mut actual = original.clone();
            for _ in 0..80 {
                let pos = r.n(actual.len() + 1);
                let mut nuevo = actual.clone();
                match if actual.is_empty() { 0 } else { r.n(4) } {
                    0 => {
                        let t = r.texto(1);
                        nuevo.splice(pos..pos, t);
                    }
                    1 => {
                        let p = pos.min(actual.len() - 1);
                        nuevo.remove(p);
                    }
                    2 => {
                        let n = 1 + r.n(7);
                        let t = r.texto(n);
                        nuevo.splice(pos..pos, t);
                    }
                    _ => {
                        let p = pos.min(actual.len() - 1);
                        let quita = 1 + r.n((actual.len() - p).min(6));
                        let n = r.n(4);
                        let t = r.texto(n);
                        nuevo.splice(p..p + quita, t);
                    }
                }
                anotar(&mut h, &actual, &nuevo);
                actual = nuevo;
                if r.n(10) == 0 {
                    for _ in 0..1 + r.n(3) {
                        actual = revertir(&mut h, &actual);
                    }
                }
            }
            while h.adelante.last().is_some() {
                actual = repetir(&mut h, &actual);
            }
            let fin = actual.clone();
            while h.atras.last().is_some() {
                actual = revertir(&mut h, &actual);
            }
            assert_eq!(actual, original, "deshacer todo vuelve al original");
            while h.adelante.last().is_some() {
                actual = repetir(&mut h, &actual);
            }
            assert_eq!(actual, fin, "rehacer todo vuelve al final");
        }
    }
}
