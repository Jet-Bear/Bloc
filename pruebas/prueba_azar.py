"""Ediciones al azar en un .md de Bloc.exe, en un escritorio invisible, buscando lo que se rompe al combinar opciones.
Uso: python prueba_azar.py <Bloc.exe> <carpeta de trabajo> [ediciones=2000] [semilla=1]

Cada pocas ediciones comprueba que lo pintado poco a poco es igual que repintarlo todo (captura, «Mostrar marcas» dos
veces, otra captura: tienen que ser idénticas), que deshacer y rehacer vuelven a un texto que existió, que un botón de
formato pulsado dos veces deja el texto como estaba y conserva la selección, que nada tarda demasiado y que no se apunta
ningún fallo. Al fallar guarda en la carpeta las capturas, el texto y las últimas ediciones, para repetirlo con la
misma semilla."""
import collections, ctypes, ctypes.wintypes as w, json, os, random, re, struct, sys, threading, time

k32, u32, g32 = ctypes.windll.kernel32, ctypes.windll.user32, ctypes.windll.gdi32
for f, res, args in (
        (u32.CreateDesktopW, w.HANDLE, [w.LPCWSTR, ctypes.c_void_p, ctypes.c_void_p, w.DWORD, w.DWORD, ctypes.c_void_p]),
        (u32.SetThreadDesktop, w.BOOL, [w.HANDLE]),
        (u32.SendMessageTimeoutW, w.LPARAM, [w.HWND, w.UINT, w.WPARAM, w.LPARAM, w.UINT, w.UINT, ctypes.POINTER(ctypes.c_size_t)]),
        (u32.GetDC, w.HDC, [w.HWND]), (u32.ReleaseDC, ctypes.c_int, [w.HWND, w.HDC]),
        (g32.CreateCompatibleDC, w.HDC, [w.HDC]), (g32.CreateCompatibleBitmap, w.HBITMAP, [w.HDC, ctypes.c_int, ctypes.c_int]),
        (g32.SelectObject, w.HGDIOBJ, [w.HDC, w.HGDIOBJ]), (g32.DeleteObject, w.BOOL, [w.HGDIOBJ]), (g32.DeleteDC, w.BOOL, [w.HDC]),
        (g32.BitBlt, w.BOOL, [w.HDC, ctypes.c_int, ctypes.c_int, ctypes.c_int, ctypes.c_int, w.HDC, ctypes.c_int, ctypes.c_int, w.DWORD]),
        (u32.RedrawWindow, w.BOOL, [w.HWND, ctypes.c_void_p, ctypes.c_void_p, w.UINT]),
        (g32.GetDIBits, ctypes.c_int, [w.HDC, w.HBITMAP, w.UINT, w.UINT, ctypes.c_void_p, ctypes.c_void_p, w.UINT]),
        (u32.GetClientRect, w.BOOL, [w.HWND, ctypes.POINTER(w.RECT)]),
        (u32.MoveWindow, w.BOOL, [w.HWND, ctypes.c_int, ctypes.c_int, ctypes.c_int, ctypes.c_int, w.BOOL]),
        (k32.OpenProcess, w.HANDLE, [w.DWORD, w.BOOL, w.DWORD]),
        (k32.VirtualAllocEx, ctypes.c_void_p, [w.HANDLE, ctypes.c_void_p, ctypes.c_size_t, w.DWORD, w.DWORD]),
        (k32.ReadProcessMemory, w.BOOL, [w.HANDLE, ctypes.c_void_p, ctypes.c_void_p, ctypes.c_size_t, ctypes.c_void_p]),
        (k32.WriteProcessMemory, w.BOOL, [w.HANDLE, ctypes.c_void_p, ctypes.c_void_p, ctypes.c_size_t, ctypes.c_void_p])):
    f.restype, f.argtypes = res, args
ENUM = ctypes.WINFUNCTYPE(w.BOOL, w.HWND, w.LPARAM)
u32.EnumDesktopWindows.argtypes = [w.HANDLE, ENUM, w.LPARAM]
u32.EnumChildWindows.argtypes = [w.HWND, ENUM, w.LPARAM]

WM_GETTEXT, WM_GETTEXTLENGTH, WM_KEYDOWN, WM_CHAR, WM_COMMAND, WM_VSCROLL = 0x0D, 0x0E, 0x100, 0x102, 0x111, 0x115
EM_GETSEL, EM_SETSEL, EM_REPLACESEL, EM_POSFROMCHAR, EM_GETCHARFORMAT, EM_SETSCROLLPOS = 0xB0, 0xB1, 0xC2, 0xD6, 0x43A, 0x4DE
VK_BACK, VK_RETURN, VK_DELETE, VK_UP, VK_DOWN = 8, 13, 0x2E, 0x26, 0x28
DESHACER, REHACER = 1010, 1011
NEGRITA, CURSIVA, TACHADO, CODIGO, ENLACE, TITULO1, TITULO2, TITULO3 = range(1030, 1038)
LISTA, NUMERADA, CASILLAS, CITA, TABLA, ALINEAR = range(1038, 1044)
AJUSTE, NOCHE, ACERCAR, ALEJAR, MARCAS = 1050, 1052, 1055, 1056, 1058
ENVOLVER = {NEGRITA: ("**", "**"), CURSIVA: ("*", "*"), TACHADO: ("~~", "~~"), CODIGO: ("`", "`")}
LENTO_MS = 400
TITULO_LINEA = re.compile(r" {0,3}#{1,6}([ \t]|$)")
LISTA_LINEA = re.compile(r"\s*([-*+]|\d{1,9}[.)])( |$)")
NUMERO_LINEA = re.compile(r"\s*(\d{1,9})[.)]( |$)")
CASILLA_SUELTA = re.compile(r"\s*\[[ xX]\]( |$)")
# Las marcas y los espacios: un código con una ` dentro va entre `` y con un espacio a cada lado (`` a`b ``), y ese
# espacio también es marca.
SIN_MARCAS = re.compile(r"[*_~`\\\s]")
MARCA_LINEA = re.compile(r"(\s*(> ?|#{1,6}[ \t]+|[-*+] (\[[ xX]\] )?|\d{1,9}[.)] ))*")

SEMILLA_DOC = """# Título uno
Texto con **negrita**, *cursiva*, ~~tachado~~, `código` y un [enlace](https://ejemplo.es).
## Título dos
- viñeta uno
  - segundo nivel con **negrita**
    - tercer nivel
	- con tabulador
* asterisco
+ más
- [ ] casilla sin marcar
- [x] casilla marcada
- [X] otra marcada
1. primero
2. segundo con una línea larga que tiene que partirse en dos o más líneas para comprobar cómo queda la sangría
3) tercero
> una cita con *cursiva*
> > cita anidada
> - lista en cita
### Título tres
```python
print("hola **no negrita**")
```
~~~
tildes
~~~
| Nombre | Edad |
|:--|--:|
| Ana | 30 |
---
![imagen](x.png) y \\*escape\\* y <https://a.es> y www.ejemplo.es
#### Cuatro
##### Cinco
###### Seis
Línea final normal"""

# Desde la semilla 13, una tabla más, de las que se dibujan: celdas largas que se parten, formato dentro y columnas
# alineadas.
TABLA_DOC = """
| Canal | Qué dice | Cómo se ve |
|---|:-:|--:|
| **Valor** (lo claro u oscuro) | La importancia y la profundidad de cada cosa en la pantalla | Fondos, paneles, `cajas` y pozos en una escala de seis valores |
| *Tono* | El significado | Estados ~~y avisos~~ |
| [enlace](x) | | 12 |"""

# En todas las semillas, párrafos, viñetas y citas partidos a mano en el archivo, que la vista junta (fase J).
PARTIDOS_DOC = """

Un párrafo largo partido a mano en el archivo, como los que escribe Claude, que sigue en la línea
de debajo sin nada delante y acaba en una tercera
línea corta.

- Una viñeta partida a mano que sigue en la línea de debajo con dos espacios de sangría, como en
  los archivos de diseño, y con **negrita** en medio.
- Otra viñeta corta.

> Una cita partida
> en dos líneas."""

TROZOS = ["a", "b", "e", "ñ", "o", " ", " ", "  ", "*", "**", "_", "~~", "`", "```", "# ", "## ", "- ", "  - ", "1. ", "2) ",
          "- [ ] ", "[x]", "> ", "|", " | ", "|---|", "---", "[", "]", "(", ")", "[a](b)", "\\", "x", "palabra", "12"]
PEGADOS = ["uno\rdos", "- a\r- b\r", "```\rcódigo\r```", "| a | b |\r|---|---|\r| 1 | 2 |", "> cita\r> sigue", "# T\rtexto **n**",
           "\r", "~~~\r", "1. x\r2. y\r3. z"]


class SI(ctypes.Structure):
    _fields_ = [("cb", w.DWORD), ("lpReserved", w.LPWSTR), ("lpDesktop", w.LPWSTR),
                ("lpTitle", w.LPWSTR), ("dwX", w.DWORD), ("dwY", w.DWORD),
                ("dwXSize", w.DWORD), ("dwYSize", w.DWORD), ("dwXCountChars", w.DWORD),
                ("dwYCountChars", w.DWORD), ("dwFillAttribute", w.DWORD), ("dwFlags", w.DWORD),
                ("wShowWindow", w.WORD), ("cbReserved2", w.WORD), ("lpReserved2", ctypes.c_void_p),
                ("hStdInput", w.HANDLE), ("hStdOutput", w.HANDLE), ("hStdError", w.HANDLE)]


class PI(ctypes.Structure):
    _fields_ = [("hProcess", w.HANDLE), ("hThread", w.HANDLE), ("dwProcessId", w.DWORD), ("dwThreadId", w.DWORD)]


class BIH(ctypes.Structure):
    _fields_ = [("biSize", w.DWORD), ("biWidth", w.LONG), ("biHeight", w.LONG), ("biPlanes", w.WORD), ("biBitCount", w.WORD),
                ("biCompression", w.DWORD), ("biSizeImage", w.DWORD), ("biXPelsPerMeter", w.LONG), ("biYPelsPerMeter", w.LONG),
                ("biClrUsed", w.DWORD), ("biClrImportant", w.DWORD)]


class Colgado(Exception):
    pass


exe, dir = sys.argv[1], sys.argv[2]
EDICIONES = int(sys.argv[3]) if len(sys.argv) > 3 else 2000
SEMILLA = int(sys.argv[4]) if len(sys.argv) > 4 else 1
ESCRITORIO = f"BlocPruebasAzar{SEMILLA}"   # uno por semilla: así pueden ir varias a la vez
os.environ["BLOC_SIN_INSTALAR"] = "1"
os.environ["APPDATA"] = dir   # errores.log va a la carpeta de la prueba, nunca a la del usuario
LOG = os.path.join(dir, "Bloc", "errores.log")
INI = os.environ["BLOC_INI"] = os.path.join(dir, "bloc-azar.ini")
with open(INI, "w") as f:
    f.write("ajuste=1\nbarra=1\nmax=0\nzoom=100\nnoche=0\ncorrector=0\nmarcas=0\n")
fallos = []
# Los últimos mensajes mandados a Bloc: si deja de responder, con qué.
ULTIMOS = collections.deque(maxlen=12)


def enviar(h, m, wp=0, lp=0, segundos=5):
    ULTIMOS.append("%#x %d %d" % (m, wp, lp))
    r = ctypes.c_size_t()
    if not u32.SendMessageTimeoutW(h, m, wp, lp, 0x2, int(segundos * 1000), ctypes.byref(r)):   # SMTO_ABORTIFHUNG
        raise Colgado("sin respuesta a %#x %d" % (m, wp))
    return r.value


def texto(h):
    # Los saltos blandos de un .md van en el control como U+2005 (md::JUNTO): el texto del archivo lleva el salto.
    return crudo(h).replace("\u2005", "\r")


def crudo(h):
    """El texto del control, con sus U+2005."""
    n = enviar(h, WM_GETTEXTLENGTH)
    b = ctypes.create_unicode_buffer(n + 1)
    enviar(h, WM_GETTEXT, n + 1, ctypes.addressof(b))
    return b.value.replace("\r\n", "\r")


def seleccion(e):
    r = enviar(e, EM_GETSEL)
    return r & 0xFFFF, (r >> 16) & 0xFFFF


def clase(h):
    b = ctypes.create_unicode_buffer(256); u32.GetClassNameW(h, b, 256); return b.value


def ventanas(escritorio, pid):
    res = []
    def cb(h, _):
        p = w.DWORD(); u32.GetWindowThreadProcessId(h, ctypes.byref(p))
        if p.value == pid and u32.IsWindowVisible(h):
            res.append(h)
        return True
    u32.EnumDesktopWindows(escritorio, ENUM(cb), 0)
    return res


def caja(h):
    res = []
    def cb(x, _):
        if "EDIT" in clase(x):
            res.append(x)
        return True
    u32.EnumChildWindows(h, ENUM(cb), 0)
    return res[0] if res else None


def largo_marca(linea):
    m = MARCA_LINEA.match(linea)
    return m.end() if m else 0


def trozo(t, otro):
    """Dónde empieza t a ser distinto de otro, y lo que hay ahí."""
    i = next((k for k, (x, y) in enumerate(zip(t, otro)) if x != y), min(len(t), len(otro)))
    return "%d:%r" % (i, t[max(0, i - 15):i + 25])


def textos_hijos(h):
    res = []
    def cb(x, _):
        n = u32.GetWindowTextLengthW(x)
        if n:
            b = ctypes.create_unicode_buffer(n + 1); u32.GetWindowTextW(x, b, n + 1); res.append(b.value)
        return True
    u32.EnumChildWindows(h, ENUM(cb), 0)
    return res


def captura(h):
    r = w.RECT(); u32.GetClientRect(h, ctypes.byref(r))
    ancho, alto = r.right, r.bottom
    dc = u32.GetDC(h); m = g32.CreateCompatibleDC(dc); b = g32.CreateCompatibleBitmap(dc, ancho, alto)
    viejo = g32.SelectObject(m, b)
    # Se copia lo que hay en pantalla: antes, lo que tenga pendiente de pintar (tras desplazarse, por ejemplo). Sin
    # invalidar nada: lo que Bloc olvide repintar tiene que seguir viéndose mal. Con BitBlt: PrintWindow saca la caja
    # de RichEdit negra o, con PW_RENDERFULLCONTENT, la hace repintarse mientras copia y salen líneas a medias.
    u32.RedrawWindow(h, None, None, 0x100 | 0x80)   # RDW_UPDATENOW | RDW_ALLCHILDREN
    g32.BitBlt(m, 0, 0, ancho, alto, dc, 0, 0, 0x00CC0020)   # SRCCOPY
    bi = BIH(ctypes.sizeof(BIH), ancho, -alto, 1, 32, 0, 0, 0, 0, 0, 0)
    buf = ctypes.create_string_buffer(ancho * alto * 4)
    g32.GetDIBits(m, b, 0, alto, buf, ctypes.byref(bi), 0)
    g32.SelectObject(m, viejo); g32.DeleteObject(b); g32.DeleteDC(m); u32.ReleaseDC(h, dc)
    datos = bytearray(buf.raw)
    datos[3::4] = bytes(ancho * alto)   # el cuarto byte no es color
    return ancho, alto, bytes(datos)


def igual(a, b):
    """Dos capturas iguales salvo el cursor, que parpadea y se desliza al escribir (en cada una, una tira de 3 píxeles de
    ancho como mucho, y no en el mismo sitio si se ha cogido a medio camino), y el redondeo de ClearType (hasta 2 de 255
    en un color: el mismo texto pintado en otro orden)."""
    if a == b:
        return True
    if a[:2] != b[:2]:
        return False
    from PIL import Image, ImageChops
    d = ImageChops.difference(*(Image.frombytes("RGB", c[:2], c[2], "raw", "RGBX") for c in (a, b)))
    marca = d.point(([0] * 3 + [255] * 253) * 3).convert("L")
    if marca.getbbox() is None:
        return True
    columnas = [x for x in range(marca.width) if marca.crop((x, 0, x + 1, marca.height)).getbbox()]
    tiras = [[columnas[0]]]
    for x in columnas[1:]:
        if x == tiras[-1][-1] + 1:
            tiras[-1].append(x)
        else:
            tiras.append([x])
    if len(tiras) <= 2 and all(len(t) <= 3 for t in tiras):
        return True
    # En cursiva el cursor va inclinado: movido un píxel, la diferencia es una raya inclinada de unos 8 de ancho, con
    # uno o dos píxeles por fila.
    x0, y0, x1, y1 = marca.getbbox()
    filas = [sum(1 for x in range(x0, x1) if marca.getpixel((x, y))) for y in range(y0, y1)]
    return x1 - x0 <= 10 and y1 - y0 <= 40 and max(filas) <= 4


class SCROLLINFO(ctypes.Structure):
    _fields_ = [("cbSize", w.UINT), ("fMask", w.UINT), ("nMin", ctypes.c_int), ("nMax", ctypes.c_int), ("nPage", w.UINT),
                ("nPos", ctypes.c_int), ("nTrackPos", ctypes.c_int)]


def guardar_png(c, ruta):
    from PIL import Image
    ancho, alto, datos = c
    Image.frombuffer("RGBX", (ancho, alto), datos, "raw", "BGRX", 0, 1).convert("RGB").save(ruta)


class Prueba:
    def __init__(self, escritorio, pi, v, e):
        self.escritorio, self.pi, self.v, self.e = escritorio, pi, v, e
        self.azar = random.Random(SEMILLA)
        self.hechas = []          # las últimas ediciones, para contar qué llevó al fallo
        self.previos = []         # BLOC_PREVIOS: el texto del control y la selección de antes de cada una de las últimas
        self.textos = {texto(e)}  # todo texto que ha existido: deshacer tiene que volver a uno de ellos
        self.n_fallo = 0
        self.proceso = k32.OpenProcess(0x0008 | 0x0010 | 0x0020, False, pi.dwProcessId)   # VM_OPERATION | VM_READ | VM_WRITE
        self.remoto = k32.VirtualAllocEx(self.proceso, None, 4096, 0x3000, 0x04)

    def fallo(self, que, capturas=()):
        self.n_fallo += 1
        base = os.path.join(dir, "fallo-s%d-%02d" % (SEMILLA, self.n_fallo))
        for i, c in enumerate(capturas):
            guardar_png(c, "%s-%d.png" % (base, i + 1))
        try:
            ahora = texto(self.e).replace("\r", "\n")
        except Colgado:
            ahora = "(Bloc no responde)"
        with open(base + ".txt", "w", encoding="utf-8") as f:
            # Los botones de vista de toda la prueba: sin ellos no se reproduce el fallo desde el texto previo.
            vistas = " ".join(h.split()[2] for h in self.hechas if h.split()[1:2] == ["ver"]) or "ninguno"
            f.write("%s\n\nÚltimas ediciones:\n%s\n\nBotones de vista: %s\n\nTexto:\n%s\n" % (que, "\n".join(self.hechas[-12:]), vistas, ahora))
        previo = os.path.join(dir, "previo.txt")
        if os.path.exists(previo):
            os.replace(previo, base + "-previo.txt")
        if self.previos:
            with open(base + "-previos.json", "w", encoding="utf-8") as f:
                json.dump(self.previos, f, ensure_ascii=False)
        fallos.append(que)
        print("FALLO  " + que + "  (" + base + ".txt)")

    def medir(self, que, accion):
        t = time.perf_counter()
        accion()
        ms = (time.perf_counter() - t) * 1000
        if ms > LENTO_MS:
            self.fallo("I7 %s tardó %.0f ms" % (que, ms))

    def pos(self):
        return self.azar.randint(0, len(texto(self.e)))

    def rango(self):
        t = texto(self.e)
        a = self.azar.randint(0, len(t))
        if self.azar.random() < 0.6:   # una palabra
            i = a
            while i > 0 and t[i - 1].isalnum():
                i -= 1
            j = a
            while j < len(t) and t[j].isalnum():
                j += 1
            return i, j
        return tuple(sorted((a, min(len(t), a + self.azar.randint(0, 40)))))

    def editar(self, m, wp=0, lp=0):
        enviar(self.e, m, wp, lp)
        self.textos.add(texto(self.e))

    def teclear(self, s):
        for c in s:
            if c == "\r":
                enviar(self.e, WM_KEYDOWN, VK_RETURN); self.editar(WM_CHAR, 13)
            else:
                self.editar(WM_CHAR, ord(c))

    def pegar(self, s):
        # Windows copia el texto de EM_REPLACESEL de quien lo envía: basta un búfer propio.
        b = ctypes.create_unicode_buffer(s)
        self.editar(EM_REPLACESEL, 1, ctypes.addressof(b))

    def boton(self, id):
        enviar(self.v, WM_COMMAND, id)
        self.textos.add(texto(self.e))

    def tabular(self, atras, en_tabla=False):
        """Tab o Mayús+Tab como del teclado. Bloc mira Mayúsculas con GetKeyState: se comparte el estado del teclado con
        su hilo. El tabulador que sigue a la tecla lo quita Bloc cuando la atiende (con Mayúsculas, con selección o en
        una lista); si no, lo escribe RichEdit."""
        e = self.e
        yo, suyo = k32.GetCurrentThreadId(), u32.GetWindowThreadProcessId(e, ctypes.byref(w.DWORD()))
        u32.AttachThreadInput(yo, suyo, True)
        teclas = (ctypes.c_ubyte * 256)()
        u32.GetKeyboardState(teclas)
        if atras:
            teclas[0x10] |= 0x80; u32.SetKeyboardState(teclas)
        try:
            a, b = seleccion(e)
            t = texto(e)
            linea = t[t.rfind("\r", 0, a) + 1:]
            self.editar(WM_KEYDOWN, 9)
            if not (atras or a != b or en_tabla or LISTA_LINEA.match(linea)):
                self.editar(WM_CHAR, 9)
        finally:
            teclas[0x10] &= 0x7F; u32.SetKeyboardState(teclas)
            u32.AttachThreadInput(yo, suyo, False)

    def en_tabla(self):
        """Algo dentro de una tabla: entrar en ella (en una celda al azar) y Tab, Mayús+Tab, Intro, escribir, sus botones,
        salir con las flechas o deshacer. Las tablas se dibujan con el cursor fuera y se ven como texto con él dentro."""
        e, az = self.e, self.azar
        lineas = texto(e).split("\r")
        filas, p = [], 0
        for k, l in enumerate(lineas):
            dentro = l.lstrip().startswith("|") and any(re.fullmatch(r"\s*\|?[\s:|-]*-[\s:|-]*", x) for x in lineas[max(0, k - 30):k + 2])
            if dentro:
                filas.append((p, len(l)))
            p += len(l) + 1
        if not filas:
            return "sin tablas"
        p, n = az.choice(filas)
        q = p + az.randint(0, n)
        enviar(e, EM_SETSEL, q, q)
        que = az.choice(["tab", "mayús+tab", "intro", "teclear", "tabla", "alinear", "abajo", "arriba", "deshacer"])
        if que in ("tab", "mayús+tab"):
            self.medir("tab", lambda: self.tabular(que != "tab", True))
        elif que == "intro":
            self.medir("intro", lambda: self.teclear("\r"))
        elif que == "teclear":
            s = az.choice(["x", "palabra", " **n** ", "12", "una frase larga que no cabe"])
            self.medir("teclear", lambda: self.teclear(s))
        elif que in ("tabla", "alinear"):
            self.medir("tabla", lambda: self.boton(TABLA if que == "tabla" else ALINEAR))
        elif que in ("abajo", "arriba"):
            for _ in range(az.randint(1, 4)):
                self.medir("flecha", lambda: self.editar(WM_KEYDOWN, VK_DOWN if que == "abajo" else VK_UP))
        else:
            self.medir("deshacer", lambda: self.boton(DESHACER))
        return "en tabla %s en %d" % (que, q)

    def una(self):
        e, az = self.e, self.azar
        t = texto(e)
        if SEMILLA >= 13 and az.random() < 0.12:
            return self.en_tabla()
        # Desde la semilla 9, también Tab y Mayús+Tab (las de antes siguen saliendo igual).
        if SEMILLA >= 9 and az.random() < 0.08:
            a, b = self.rango() if az.random() < 0.3 else ((p := self.pos()), p)
            atras = az.random() < 0.4
            enviar(e, EM_SETSEL, a, b)
            self.medir("tab", lambda: self.tabular(atras))
            return "%s %d-%d" % ("mayús+tab" if atras else "tab", a, b)
        x = az.random()
        if len(t) > 2500 and x < 0.3:
            a, b = sorted((az.randint(0, len(t)), az.randint(0, len(t))))
            enviar(e, EM_SETSEL, a, b); self.editar(WM_KEYDOWN, VK_DELETE)
            return "borrar %d-%d" % (a, b)
        if x < 0.30:
            p, s = self.pos(), "".join(az.choice(TROZOS) for _ in range(az.randint(1, 3)))
            enviar(e, EM_SETSEL, p, p)
            self.medir("teclear", lambda: self.teclear(s))
            return "teclear %r en %d" % (s, p)
        if x < 0.40:
            p = self.pos(); enviar(e, EM_SETSEL, p, p)
            self.medir("intro", lambda: self.teclear("\r"))
            return "intro en %d" % p
        if x < 0.52:
            a, b = self.rango() if az.random() < 0.3 else ((p := self.pos()), p)
            tecla = az.choice([VK_BACK, VK_DELETE])
            enviar(e, EM_SETSEL, a, b)
            self.medir("borrar", lambda: self.editar(WM_KEYDOWN, tecla))
            return "%s %d-%d" % ("retroceso" if tecla == VK_BACK else "supr", a, b)
        if x < 0.70:
            return self.formato()
        if x < 0.80:
            n = az.randint(1, 4)
            for _ in range(n):
                self.medir("deshacer", lambda: self.boton(DESHACER))
            if texto(e) not in self.textos:
                self.fallo("I3 deshacer %d veces deja un texto que nunca existió" % n)
            antes = texto(e)
            self.boton(REHACER)
            rehecho = texto(e)
            self.boton(DESHACER)
            if texto(e) != antes:
                self.fallo("I3 rehacer y deshacer no vuelve al mismo texto: antes %s / rehecho %s / después %s" % (
                    trozo(antes, rehecho), trozo(rehecho, antes), trozo(texto(e), antes)))
            return "deshacer x%d" % n
        if x < 0.86:
            a, b = self.rango(); s = az.choice(PEGADOS)
            self.ultima = "pegar %r en %d-%d" % (s, a, b)
            enviar(e, EM_SETSEL, a, b)
            self.medir("pegar", lambda: self.pegar(s))
            return "pegar %r en %d-%d" % (s, a, b)
        if x < 0.89:
            p = self.pos(); enviar(e, EM_SETSEL, p, p)
            id = az.choice([TABLA, ALINEAR])
            self.medir("tabla", lambda: self.boton(id))
            return "tabla" if id == TABLA else "alinear"
        id = az.choice([AJUSTE, NOCHE, ACERCAR, ALEJAR, MARCAS])
        self.medir("ver", lambda: self.boton(id))
        return "ver %d" % id

    def formato(self):
        e, az = self.e, self.azar
        id = az.choice(list(ENVOLVER) + [ENLACE, TITULO1, TITULO2, TITULO3, LISTA, NUMERADA, CASILLAS, CITA])
        a, b = self.rango()
        antes = texto(e)
        enviar(e, EM_SETSEL, a, b)
        # La selección que puede tener quien escribe: RichEdit la saca de dentro del texto oculto (las marcas de dentro del
        # texto, con las marcas ocultas).
        a, b = seleccion(e)
        control = crudo(e)
        self.medir("formato", lambda: self.boton(id))
        despues = texto(e)
        sa, sb = seleccion(e)
        sel = antes[a:b]
        nombre = "formato %d en %d-%d %r" % (id, a, b, sel)
        ini = antes.rfind("\r", 0, a) + 1
        fin = antes.find("\r", max(a, b - 1))
        fin = len(antes) if fin < 0 else fin
        lineas = antes[ini:fin].split("\r")
        # Los botones de lista y de cita cogen enteros los párrafos juntados: sus líneas van tras un U+2005 en el control.
        bi, bf = ini, fin
        while bi > 0 and control[bi - 1] == " ":
            bi = antes.rfind("\r", 0, bi - 1) + 1
        while bf < len(antes) and control[bf] == " ":
            bf = antes.find("\r", bf + 1)
            bf = len(antes) if bf < 0 else bf
        bloque = antes[bi:bf].split("\r")
        bloque_sin_cita = [re.sub(r"^( {0,3}> ?)+", "", l) for l in bloque]
        if id in ENVOLVER:
            # Sin contar marcas, lo seleccionado sigue seleccionado (si tocaba un enlace o un código, entero).
            if SIN_MARCAS.sub("", sel) not in SIN_MARCAS.sub("", despues[sa:sb]):
                self.fallo("I5 %s: tras el formato la selección es %r" % (nombre, despues[sa:sb]))
        elif id != ENLACE and a < b and a - ini >= largo_marca(lineas[0]) \
                and b - antes.rfind("\r", 0, b) - 1 >= largo_marca(lineas[-1]):
            # Las líneas cambian de marca, pero lo seleccionado sigue seleccionado.
            primera, ultima = sel.split("\r")[0], sel.split("\r")[-1]
            if "\r" in sel:
                ultima = ultima[largo_marca(ultima):].lstrip()   # la marca nueva va tras la sangría
            hay = despues[sa:sb]
            desde, entera = hay, sel
            if id in (TITULO1, TITULO2, TITULO3, LISTA, NUMERADA, CASILLAS) and a - ini <= len(lineas[0]) - len(lineas[0].lstrip(" \t")):
                # El título quita la sangría de delante (una fila de tabla, que no cambia, la conserva), y la marca de lista
                # va tras ella: lo seleccionado de la sangría se queda fuera.
                primera, desde, entera = primera.lstrip(" \t"), hay.lstrip(" \t"), sel.lstrip(" \t")
                if "\r" not in sel:
                    ultima = primera
            if id in (TITULO1, TITULO2, TITULO3) and len(lineas) == 1:
                bien = hay.strip() == sel.strip()   # parte de una línea pasa a la suya, sin los espacios de los bordes
            else:
                bien = desde.startswith(primera) and hay.endswith(ultima) and (len(lineas) > 1 or desde == entera)
            if not bien:
                self.fallo("I5 %s: tras el formato la selección es %r" % (nombre, hay))
        # Pulsado otra vez, lo deshace. Un título o una lista de otro tipo se cambian en vez de quitarse: ahí no vale. Un
        # título y una lista se cambian el uno por la otra, un título sobre parte de una línea la parte en tres y quita la
        # sangría de delante.
        sin_cita = [re.sub(r"^( {0,3}> ?)+", "", l) for l in lineas]
        if id in (TITULO1, TITULO2, TITULO3):
            parte = len(lineas) == 1 and a < b and sel.strip() != lineas[0][largo_marca(lineas[0]):].strip()
            # Un título sobre un párrafo juntado (algún U+2005 en el control) junta sus líneas: quitarlo no las parte.
            juntado = "\u2005" in control[max(ini - 1, 0):fin + 1]
            vale = not parte and not juntado and not any(TITULO_LINEA.match(l) or LISTA_LINEA.match(l) or l[:1] in (" ", "\t") for l in sin_cita)
        elif id in (LISTA, NUMERADA, CASILLAS):
            # «[ ] x» con «- » delante ya es una casilla, y pulsar otra vez la cambia en vez de quitarla.
            vale = not any(LISTA_LINEA.match(l) or CASILLA_SUELTA.match(l) or TITULO_LINEA.match(l) for l in bloque_sin_cita)
        elif id in ENVOLVER:
            # Con otras marcas en la línea, poner y quitar puede unir o partir parejas: eso lo prueba cargo test.
            vale = not any(c in "*_~`\\[]<" for c in antes[ini:fin])
        elif id == CITA:
            # «>x» se quita y vuelve como «> x». Dentro de una cita Bloc no junta las líneas de una lista: citar un párrafo
            # juntado que lleva una lo parte, y quitar la cita ya no lo coge entero. Y un número que no es 1 vuelve
            # numerado desde 1 (md::renumerar: lo que se mueve empieza su lista).
            vale = not any(l.lstrip().startswith(">") for l in bloque) \
                and not (" " in control[bi:bf] and any(LISTA_LINEA.match(l) for l in bloque_sin_cita)) \
                and not any(m and m.group(1) != "1" for m in map(NUMERO_LINEA.match, bloque_sin_cita))
        else:
            vale = id != ENLACE
        if vale:
            self.boton(id)
            otra = texto(e)
            if otra != antes:
                with open(os.path.join(dir, "previo.txt"), "w", encoding="utf-8", newline="") as f:
                    f.write(control)   # para reproducirlo: el texto de antes, con sus U+2005
                k = next((i for i, (x, y) in enumerate(zip(antes, otra)) if x != y), min(len(antes), len(otra)))
                self.fallo("I4 %s: pulsado dos veces no deja el texto como estaba: en %d había %r y hay %r"
                           % (nombre, k, antes[max(0, k - 20):k + 30], otra[max(0, k - 20):k + 30]))
        return nombre

    def alturas(self):
        """Dónde empieza cada línea, en píxeles desde arriba del documento, y dónde acaba la última."""
        enviar(self.e, WM_VSCROLL, 6)
        t = texto(self.e)
        inicios = [0] + [i + 1 for i, c in enumerate(t) if c == "\r"] + [len(t)]
        res = []
        for i in inicios:
            enviar(self.e, EM_POSFROMCHAR, self.remoto, i)   # no se copia entre procesos: va a la memoria de Bloc
            p = (ctypes.c_long * 2)()
            k32.ReadProcessMemory(self.proceso, self.remoto, p, 8, None)
            res.append(p[1])
        return res

    def saltos(self):
        """El tamaño de letra (en twips) de cada salto de línea y de la marca de párrafo del final."""
        t = texto(self.e)
        sel = seleccion(self.e)
        res = []
        for p in [i for i, c in enumerate(t) if c == "\r"] + [len(t)]:
            enviar(self.e, EM_SETSEL, p, p + 1)
            cf = (ctypes.c_byte * 116)()
            struct.pack_into("<II", cf, 0, 116, 0x80000000)   # cbSize, CFM_SIZE
            k32.WriteProcessMemory(self.proceso, self.remoto + 64, cf, 116, None)
            enviar(self.e, EM_GETCHARFORMAT, 1, self.remoto + 64)   # SCF_SELECTION
            k32.ReadProcessMemory(self.proceso, self.remoto + 64, cf, 116, None)
            res.append(struct.unpack_from("<i", cf, 12)[0])
        enviar(self.e, EM_SETSEL, *sel)
        return res

    def paginas(self):
        """El alto del documento (el de su barra de desplazamiento) y sus capturas, página a página. La ventana es más
        baja que el documento: así siempre hay barra (si no, con el ajuste de línea, RichEdit la pone o la quita según
        por dónde venga)."""
        si = SCROLLINFO(ctypes.sizeof(SCROLLINFO), 0x17)   # SIF_ALL
        u32.GetScrollInfo(self.e, 1, ctypes.byref(si))
        res, y = [], 0
        for _ in range(60):
            k32.WriteProcessMemory(self.proceso, self.remoto, (ctypes.c_long * 2)(0, y), 8, None)
            enviar(self.e, EM_SETSCROLLPOS, 0, self.remoto)   # tampoco se copia: va en la memoria de Bloc
            res.append(captura(self.e))
            if not si.nPage or y + si.nPage > si.nMax:
                break
            y += si.nPage
        return (si.nMax, si.nPage), res

    def visible(self):
        """El texto sin lo oculto (EM_GETTEXTEX con GT_NOHIDDENTEXT): sin mover la selección, que haría a Bloc enseñar
        marcas o abrir tablas."""
        largo = 2 * len(texto(self.e)) + 64
        if getattr(self, "remoto_texto", (0, 0))[1] < largo:
            self.remoto_texto = (k32.VirtualAllocEx(self.proceso, None, 2 * largo, 0x3000, 0x04), 2 * largo)
        r, n = self.remoto_texto
        k32.WriteProcessMemory(self.proceso, r, struct.pack("<IIIIQQ", n - 32, 8, 1200, 0, 0, 0), 32, None)
        enviar(self.e, 0x45E, r, r + 32)   # EM_GETTEXTEX
        buf = ctypes.create_string_buffer(n - 32)
        k32.ReadProcessMemory(self.proceso, r + 32, buf, n - 32, None)
        return buf.raw.decode("utf-16-le", "replace").split("\0", 1)[0]

    def comparar(self):
        """I2: lo pintado poco a poco tiene que verse igual que repintado entero."""
        # saltos() mueve la selección: si sale de un párrafo con las marcas enseñadas (el cursor dentro de unas ocultas),
        # Bloc las vuelve a esconder con un mensaje que se atiende después. Va antes de las capturas y se le deja acabar.
        saltos_antes = self.saltos()
        time.sleep(0.05)
        enviar(self.v, 0)
        # alturas() va antes: obliga a RichEdit a colocar todas las líneas (si no, el alto de lo no visto es una estimación).
        alto_antes, (rango_antes, antes), visto_antes = self.alturas(), self.paginas(), self.visible()
        self.boton(MARCAS); self.boton(MARCAS)
        alto_despues, (rango_despues, despues), saltos_despues = self.alturas(), self.paginas(), self.saltos()
        visto_despues = self.visible()
        distintas = [i for i, (a, b) in enumerate(zip(antes, despues)) if not igual(a, b)]
        if rango_antes != rango_despues or distintas or len(antes) != len(despues) or alto_antes != alto_despues:
            k = distintas[0] if distintas else min(len(antes), len(despues)) - 1
            lineas = texto(self.e).split("\r") + ["(fin)"]
            l = next((i for i, (a, b) in enumerate(zip(alto_antes, alto_despues)) if a != b), None)
            donde = "todas las líneas en su sitio" if l is None else "la línea %d (%r) empieza en %d y repintada en %d" % (
                l, lineas[l - 1 if l else 0][:60], alto_antes[l], alto_despues[l])
            saltos = [(i, lineas[i][:30], a, b) for i, (a, b) in enumerate(zip(saltos_antes, saltos_despues)) if a != b]
            oculto = "lo oculto igual" if visto_antes == visto_despues else "oculto distinto: %r / %r" % (
                trozo(visto_antes, visto_despues), trozo(visto_despues, visto_antes))
            self.fallo("I2 lo pintado no es igual que repintado entero (alto %s / %s; páginas %s de %d/%d; %s; saltos con otra letra %s; "
                       "%s; acaba en %r)" % (rango_antes, rango_despues, distintas, len(antes), len(despues), donde, saltos[:4],
                                             oculto, texto(self.e)[-30:]), (antes[k], despues[k]))

    def correr(self):
        u32.MoveWindow(self.v, 0, 0, 1000, 450, True)
        t0 = time.perf_counter()
        hasta_comparar = 1
        for n in range(EDICIONES):
            try:
                # BLOC_COMPARAR_SIEMPRE=N: desde la edición N (sin número, desde la primera); antes, el mismo camino.
                siempre = os.environ.get("BLOC_COMPARAR_SIEMPRE") is not None and n >= int(os.environ["BLOC_COMPARAR_SIEMPRE"] or 0)
                if siempre:   # el texto de antes de cada edición, por si es la que falla
                    with open(os.path.join(dir, "previo.txt"), "w", encoding="utf-8", newline="") as f:
                        f.write(crudo(self.e))   # con sus U+2005: así se reproduce con las líneas juntadas
                if os.environ.get("BLOC_PREVIOS"):   # sin comparar de más: la prueba sigue el mismo camino
                    self.previos = self.previos[-7:] + [(n, crudo(self.e), seleccion(self.e))]
                hecho = self.una()
                self.hechas.append("%d %s" % (n, hecho))
                self.textos.add(texto(self.e))
                if os.environ.get("BLOC_SALTOS"):   # I8, para buscar fallos: cada salto con la letra del texto normal
                    saltos = self.saltos()
                    malos = [i for i, x in enumerate(saltos[:-1]) if x != 220]
                    if malos:
                        ls = texto(self.e).split("\r")
                        self.fallo("I8 tras «%s», saltos con otra letra en las líneas %s de %d: %s" % (hecho, malos[:8], len(ls),
                                   [(ls[i - 1][:25], ls[i][:25], saltos[i], ls[i + 1][:25] if i + 1 < len(ls) else None) for i in malos[:2]]))
                hasta_comparar -= 1
                if hasta_comparar == 0 or siempre:   # para buscar qué edición lo rompe
                    self.comparar()
                    if hasta_comparar <= 0:
                        hasta_comparar = self.azar.randint(1, 6)
                if os.path.exists(LOG):
                    self.fallo("I6 errores.log: " + open(LOG, encoding="utf-8", errors="replace").read()[-500:])
                    os.remove(LOG)
            except Colgado as c:
                vivo = k32.WaitForSingleObject(self.pi.hProcess, 0) != 0
                avisos = [h for h in ventanas(self.escritorio, self.pi.dwProcessId) if clase(h) == "#32770"]
                que = "; ".join(t for h in avisos for t in textos_hijos(h)) or "sin avisos"
                self.hechas.append("%d %s" % (n, getattr(self, "ultima", "?")))
                self.fallo("I6 Bloc no responde (%s), %s: %s; últimos mensajes: %s" % (c, "sigue abierto" if vivo else "se ha cerrado", que,
                                                                                     ", ".join(ULTIMOS)))
                return
            if len(fallos) >= 15:
                print("Demasiados fallos: se para.")
                return
        print("%d ediciones en %.0f s" % (EDICIONES, time.perf_counter() - t0))


def principal():
    escritorio = u32.CreateDesktopW(ESCRITORIO, None, None, 0, 0x10000000, None)
    u32.SetThreadDesktop(escritorio)
    ruta = os.path.join(dir, "azar.md")
    with open(ruta, "w", encoding="utf-8", newline="\n") as f:
        f.write(SEMILLA_DOC + (TABLA_DOC if SEMILLA >= 13 else "") + PARTIDOS_DOC + "\n")
    if os.path.exists(LOG):
        os.remove(LOG)
    si = SI(); si.cb = ctypes.sizeof(SI); si.lpDesktop = ESCRITORIO
    pi = PI()
    if not k32.CreateProcessW(None, ctypes.create_unicode_buffer('"%s" "%s"' % (exe, ruta)), None, None, False, 0, None, None,
                              ctypes.byref(si), ctypes.byref(pi)):
        sys.exit("CreateProcess: %d" % k32.GetLastError())
    u32.WaitForInputIdle(pi.hProcess, 15000)
    try:
        v = None
        for _ in range(160):
            v = next((h for h in ventanas(escritorio, pi.dwProcessId) if clase(h) != "#32770" and caja(h)), None)
            if v:
                break
            time.sleep(0.05)
        if not v:
            sys.exit("No sale la ventana")
        try:
            Prueba(escritorio, pi, v, caja(v)).correr()
        except Exception as x:
            fallos.append("la prueba se ha roto: %r" % x)   # que no acabe en «0 fallos»
            raise
    finally:
        k32.TerminateProcess(pi.hProcess, 0)


hilo = threading.Thread(target=principal)
hilo.start(); hilo.join()
print("%d fallos (semilla %d)" % (len(fallos), SEMILLA))
sys.exit(1 if fallos else 0)
