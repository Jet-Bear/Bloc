"""Prueba el pegado con formato de Bloc.exe con el portapapeles de verdad, en un escritorio invisible.
El portapapeles es el de quien usa el PC (un escritorio oculto lo comparte): lo de la prueba está en él solo el rato de
cada pegado, y justo después se repone lo suyo, salvo que mientras tanto haya copiado otra cosa, que se deja. No entra en
el historial de Win+V, y si lo suyo es algo que no se sabe reponer (una imagen, archivos), la prueba no se hace.
Uso: python prueba_pegar.py <Bloc.exe> <carpeta de trabajo>"""
import ctypes, ctypes.wintypes as w, os, sys, threading, time

k32, u32 = ctypes.windll.kernel32, ctypes.windll.user32
for f, res, args in ((u32.CreateDesktopW, w.HANDLE, [w.LPCWSTR, ctypes.c_void_p, ctypes.c_void_p, w.DWORD, w.DWORD, ctypes.c_void_p]),
                     (u32.SendMessageW, w.LPARAM, [w.HWND, w.UINT, w.WPARAM, w.LPARAM]),
                     (u32.PostMessageW, w.BOOL, [w.HWND, w.UINT, w.WPARAM, w.LPARAM]), (u32.OpenClipboard, w.BOOL, [w.HWND]),
                     (u32.GetClipboardData, ctypes.c_void_p, [w.UINT]), (u32.SetClipboardData, ctypes.c_void_p, [w.UINT, ctypes.c_void_p]),
                     (u32.EnumClipboardFormats, w.UINT, [w.UINT]), (u32.RegisterClipboardFormatW, w.UINT, [w.LPCWSTR]),
                     (u32.GetClipboardSequenceNumber, w.DWORD, []), (k32.GlobalAlloc, ctypes.c_void_p, [w.UINT, ctypes.c_size_t]),
                     (k32.GlobalLock, ctypes.c_void_p, [ctypes.c_void_p]), (k32.GlobalUnlock, w.BOOL, [ctypes.c_void_p]),
                     (k32.GlobalSize, ctypes.c_size_t, [ctypes.c_void_p])):
    f.restype, f.argtypes = res, args
ENUM = ctypes.WINFUNCTYPE(w.BOOL, w.HWND, w.LPARAM)
u32.EnumDesktopWindows.argtypes = [w.HANDLE, ENUM, w.LPARAM]
u32.EnumChildWindows.argtypes = [w.HWND, ENUM, w.LPARAM]
WM_GETTEXT, WM_GETTEXTLENGTH, WM_CLOSE, WM_COMMAND, EM_GETSEL, EM_SETSEL, IDNO = 0x0D, 0x0E, 0x10, 0x111, 0xB0, 0xB1, 7
PEGAR, PEGAR_TEXTO, DESHACER = 1014, 1023, 1010
# Lo que no es un bloque de memoria y no se sabe copiar: mapas de bits, metarchivos, paletas, dibujos del dueño.


class SI(ctypes.Structure):
    _fields_ = [("cb", w.DWORD), ("lpReserved", w.LPWSTR), ("lpDesktop", w.LPWSTR),
                ("lpTitle", w.LPWSTR), ("dwX", w.DWORD), ("dwY", w.DWORD),
                ("dwXSize", w.DWORD), ("dwYSize", w.DWORD), ("dwXCountChars", w.DWORD),
                ("dwYCountChars", w.DWORD), ("dwFillAttribute", w.DWORD), ("dwFlags", w.DWORD),
                ("wShowWindow", w.WORD), ("cbReserved2", w.WORD), ("lpReserved2", ctypes.c_void_p),
                ("hStdInput", w.HANDLE), ("hStdOutput", w.HANDLE), ("hStdError", w.HANDLE)]


class PI(ctypes.Structure):
    _fields_ = [("hProcess", w.HANDLE), ("hThread", w.HANDLE), ("dwProcessId", w.DWORD), ("dwThreadId", w.DWORD)]


ESCRITORIO = "BlocPruebasPegar"
escritorio = u32.CreateDesktopW(ESCRITORIO, None, None, 0, 0x10000000, None)
exe, dir = sys.argv[1], sys.argv[2]
os.environ["BLOC_SIN_INSTALAR"] = "1"
os.environ["APPDATA"] = dir   # errores.log va a la carpeta de la prueba, nunca a la del usuario
INI = os.environ["BLOC_INI"] = os.path.join(dir, "bloc-pegar.ini")
with open(INI, "w") as f:
    f.write("ajuste=1\nbarra=1\nmax=0\nzoom=100\nnoche=0\n")
fallos = 0


def ok(bien, que):
    global fallos
    print(("ok     " if bien else "FALLO  ") + que)
    fallos += 0 if bien else 1


def abrir_portapapeles():
    return esperar(lambda: u32.OpenClipboard(None), 2)


def texto_portapapeles():
    if not abrir_portapapeles():
        return None
    try:
        m = u32.GetClipboardData(13)   # CF_UNICODETEXT
        p = k32.GlobalLock(m) if m else None
        t = ctypes.wstring_at(p) if p else None
        if p:
            k32.GlobalUnlock(m)
        return t
    finally:
        u32.CloseClipboard()


NO_MEMORIA = {2, 3, 9, 14, 0x80, 0x82, 0x83, 0x8E} | set(range(0x300, 0x400))


class Saltada(Exception):
    pass


def guardar_portapapeles():
    """Todo lo que hay, formato a formato; None si hay algo que no se sabe reponer."""
    if not abrir_portapapeles():
        return None
    guardado, f = [], 0
    try:
        while True:
            f = u32.EnumClipboardFormats(f)
            if not f:
                return guardado
            if f in NO_MEMORIA:
                return None
            m = u32.GetClipboardData(f)
            p = k32.GlobalLock(m) if m else None
            if not p:
                return None
            guardado.append((f, ctypes.string_at(p, k32.GlobalSize(m))))
            k32.GlobalUnlock(m)
    finally:
        u32.CloseClipboard()


class en_portapapeles:
    """Con `formatos` en el portapapeles (None: sin poner nada) mientras dura el bloque; al salir, lo de quien usa el PC
    otra vez, si nadie ha copiado nada mientras. Lo que cambia la propia prueba (Bloc al copiar) se marca con `hecho`."""

    def __init__(self, formatos):
        self.formatos = formatos

    def __enter__(self):
        self.suyo = guardar_portapapeles()
        if self.suyo is None:
            raise Saltada
        self.marca = u32.GetClipboardSequenceNumber() if self.formatos is None else poner_portapapeles(self.formatos)
        return self

    def hecho(self):
        self.marca = u32.GetClipboardSequenceNumber()

    def __exit__(self, *_):
        if u32.GetClipboardSequenceNumber() == self.marca:
            poner_portapapeles(self.suyo)
        else:
            print("(mientras tanto se ha copiado otra cosa: se deja)")


def poner_portapapeles(formatos):
    abrir_portapapeles()
    u32.EmptyClipboard()
    for f, b in formatos:
        m = k32.GlobalAlloc(0x2, max(len(b), 1))
        ctypes.memmove(k32.GlobalLock(m), b, len(b))
        k32.GlobalUnlock(m)
        u32.SetClipboardData(f, m)
    u32.CloseClipboard()
    return u32.GetClipboardSequenceNumber()


def archivos(*rutas):
    """Lo que deja el Explorador al copiar archivos: CF_HDROP (DROPFILES con las rutas en UTF-16). Fuera del historial."""
    datos = (20).to_bytes(4, "little") + bytes(12) + (1).to_bytes(4, "little")
    datos += "".join(r + "\0" for r in rutas).encode("utf-16-le") + "\0".encode("utf-16-le")
    return [(15, datos), (u32.RegisterClipboardFormatW("ExcludeClipboardContentFromMonitorProcessing"), b"\0")]


def copiar(html, plano):
    """Lo que deja un navegador: el trozo en «HTML Format» (UTF-8, con su cabecera) y el texto. Fuera del historial."""
    cab = "Version:0.9\r\nStartHTML:-1\r\nEndHTML:-1\r\nStartFragment:%010d\r\nEndFragment:%010d\r\n"
    base = "<html><body><!--StartFragment-->"
    ini = len((cab % (0, 0)).encode()) + len(base.encode())
    datos = (cab % (ini, ini + len(html.encode()))).encode() + (base + html + "<!--EndFragment--></body></html>").encode() + b"\0"
    fuera = u32.RegisterClipboardFormatW("ExcludeClipboardContentFromMonitorProcessing")
    return [(u32.RegisterClipboardFormatW("HTML Format"), datos), (13, (plano + "\0").encode("utf-16-le")), (fuera, b"\0")]


def clase(h):
    b = ctypes.create_unicode_buffer(256); u32.GetClassNameW(h, b, 256); return b.value


def texto(h):
    n = u32.SendMessageW(h, WM_GETTEXTLENGTH, 0, 0)
    b = ctypes.create_unicode_buffer(n + 1)
    u32.SendMessageW(h, WM_GETTEXT, n + 1, ctypes.addressof(b))
    # Los saltos blandos de un .md van en el control como U+2005 (md::JUNTO): el texto del archivo lleva el salto.
    return b.value.replace(" ", "\r\n")


def ventanas(pid):
    res = []
    def cb(h, _):
        p = w.DWORD(); u32.GetWindowThreadProcessId(h, ctypes.byref(p))
        if p.value == pid and u32.IsWindowVisible(h):
            res.append(h)
        return True
    u32.EnumDesktopWindows(escritorio, ENUM(cb), 0)
    return res


def esperar(cond, segundos=8):
    fin = time.time() + segundos
    while time.time() < fin:
        r = cond()
        if r:
            return r
        time.sleep(0.05)
    return None


def abrir(ruta):
    si = SI(); si.cb = ctypes.sizeof(SI); si.lpDesktop = ESCRITORIO
    pi = PI()
    orden = '"%s"' % exe if ruta is None else '"%s" "%s"' % (exe, ruta)   # sin archivo: un documento nuevo
    k32.CreateProcessW(None, ctypes.create_unicode_buffer(orden), None, None, False, 0, None, None,
                       ctypes.byref(si), ctypes.byref(pi))
    u32.WaitForInputIdle(pi.hProcess, 15000)
    v = esperar(lambda: next((h for h in ventanas(pi.dwProcessId) if clase(h) == "BlocVentana"), None))
    hijos = []
    u32.EnumChildWindows(v, ENUM(lambda x, _: hijos.append(x) or True), 0)
    return pi, v, next(x for x in hijos if "EDIT" in clase(x))


def seleccion(e):
    r = u32.SendMessageW(e, EM_GETSEL, 0, 0)
    return r & 0xFFFF, (r >> 16) & 0xFFFF


def cerrar_sin_guardar(pi, v):
    u32.PostMessageW(v, WM_CLOSE, 0, 0)
    d = esperar(lambda: next((h for h in ventanas(pi.dwProcessId) if clase(h) == "#32770"), None), 3)
    if d:
        u32.PostMessageW(d, WM_COMMAND, IDNO, 0)
    acabado = k32.WaitForSingleObject(pi.hProcess, 6000) == 0
    if not acabado:
        k32.TerminateProcess(pi.hProcess, 1)
    return acabado


def pruebas():
    original = "Antes\r\n\r\n```\r\ncodigo\r\n```\r\n"
    ruta = os.path.join(dir, "pegar.md")
    with open(ruta, "w", encoding="utf-8", newline="") as f:
        f.write(original)
    F = copiar("<h2>Hola</h2><p>con <b>negrita</b> y <a href=\"https://e.es\">enlace</a></p>", "Hola\r\ncon negrita y enlace")
    pi, v, e = abrir(ruta)
    u32.SendMessageW(e, EM_SETSEL, 0, 0)
    with en_portapapeles(F):
        u32.SendMessageW(v, WM_COMMAND, PEGAR, 0)
    ok(texto(e).startswith("## Hola\r\n\r\ncon **negrita** y [enlace](https://e.es)Antes"), "md: pega lo copiado con formato como Markdown")
    u32.SendMessageW(v, WM_COMMAND, DESHACER, 0)
    ok(texto(e) == original, "md: un «Deshacer» quita todo lo pegado")
    u32.SendMessageW(e, EM_SETSEL, 0, 0)
    with en_portapapeles(F):
        u32.SendMessageW(v, WM_COMMAND, PEGAR_TEXTO, 0)
    ok(texto(e).startswith("Hola\r\ncon negrita y enlaceAntes"), "md: «Pegar sin formato» pega el texto")
    u32.SendMessageW(v, WM_COMMAND, DESHACER, 0)
    u32.SendMessageW(e, EM_SETSEL, 11, 11)   # dentro del bloque de código (los saltos cuentan uno)
    with en_portapapeles(F):
        u32.SendMessageW(v, WM_COMMAND, PEGAR, 0)
    ok("```\r\nHola\r\ncon negrita y enlacecodigo" in texto(e), "md: dentro de un bloque de código pega el texto")
    ok(cerrar_sin_guardar(pi, v), "md: se cierra bien")

    ruta_txt = os.path.join(dir, "pegar.txt")
    with open(ruta_txt, "w", encoding="utf-8", newline="") as f:
        f.write("Antes")
    pi, v, e = abrir(ruta_txt)
    u32.SendMessageW(e, EM_SETSEL, 0, 0)
    with en_portapapeles(F):
        u32.SendMessageW(v, WM_COMMAND, PEGAR, 0)
    ok(texto(e) == "Hola\r\ncon negrita y enlaceAntes", "txt: pega siempre el texto")
    ok(cerrar_sin_guardar(pi, v), "txt: se cierra bien")

    F = copiar("<span>sin formato</span>", "solo texto")
    pi, v, e = abrir(ruta)
    u32.SendMessageW(e, EM_SETSEL, 0, 0)
    with en_portapapeles(F):
        u32.SendMessageW(v, WM_COMMAND, PEGAR, 0)
    ok(texto(e).startswith("solo textoAntes"), "md: si lo copiado no trae formato, pega el texto")
    ok(cerrar_sin_guardar(pi, v), "md sin formato: se cierra bien")

    # Una dirección pegada sobre un trozo de una línea de un Markdown lo convierte en su enlace (L13).
    fuera = u32.RegisterClipboardFormatW("ExcludeClipboardContentFromMonitorProcessing")
    F = [(13, "  https://e.es/a?b=1  \0".encode("utf-16-le")), (fuera, b"\0")]
    lineas = ["Mira la web", "http://viejo.es", "dos", "lineas", "```", "web", "```", "", "| a | b |", "|---|---|", "| x | y |", "", "[texto](http://v.es)"]
    doc = "\r\n".join(lineas)
    ruta_enlace = os.path.join(dir, "enlace.md")
    with open(ruta_enlace, "w", encoding="utf-8", newline="") as f:
        f.write(doc)

    def pos(n, col):
        return sum(len(x) + 1 for x in lineas[:n]) + col

    pi, v, e = abrir(ruta_enlace)
    u32.SendMessageW(e, EM_SETSEL, pos(0, 8), pos(0, 11))
    with en_portapapeles(F):
        u32.SendMessageW(v, WM_COMMAND, PEGAR, 0)
    fin = pos(0, 8) + len("[web](https://e.es/a?b=1)")
    ok(texto(e).startswith("Mira la [web](https://e.es/a?b=1)\r\n") and seleccion(e) == (fin, fin),
       "md: pegar una dirección sobre lo seleccionado hace el enlace, con el cursor tras el paréntesis")
    u32.SendMessageW(v, WM_COMMAND, DESHACER, 0)
    ok(texto(e) == doc and seleccion(e) == (pos(0, 8), pos(0, 11)), "md: un «Deshacer» deja el texto seleccionado como estaba")
    for que, desde, hasta, orden in (("con «Pegar sin formato»", pos(0, 8), pos(0, 11), PEGAR_TEXTO),
                                     ("si lo seleccionado ya es una dirección", pos(1, 0), pos(1, 15), PEGAR),
                                     ("si lo seleccionado ocupa más de una línea", pos(2, 0), pos(3, 6), PEGAR),
                                     ("en un bloque de código", pos(5, 0), pos(5, 3), PEGAR),
                                     ("si coge una barra de tabla", pos(8, 2), pos(8, 7), PEGAR),
                                     ("sin nada seleccionado", pos(0, 8), pos(0, 8), PEGAR),
                                     ("si es el texto de un enlace", pos(12, 1), pos(12, 6), PEGAR)):
        u32.SendMessageW(e, EM_SETSEL, desde, hasta)
        with en_portapapeles(F):
            u32.SendMessageW(v, WM_COMMAND, orden, 0)
        ok(texto(e).count("](") == doc.count("](") and "https://e.es/a?b=1" in texto(e), "md: se pega la dirección tal cual %s" % que)
        u32.SendMessageW(v, WM_COMMAND, DESHACER, 0)
        ok(texto(e) == doc, "md: y «Deshacer» lo deja igual (%s)" % que)
    F = [(13, "https://e.es/a b\0".encode("utf-16-le")), (fuera, b"\0")]
    u32.SendMessageW(e, EM_SETSEL, pos(0, 8), pos(0, 11))
    with en_portapapeles(F):
        u32.SendMessageW(v, WM_COMMAND, PEGAR, 0)
    ok(texto(e).startswith("Mira la https://e.es/a b\r\n"), "md: con un espacio dentro no es una dirección, y se pega tal cual")
    ok(cerrar_sin_guardar(pi, v), "md con enlace: se cierra bien")
    F = [(13, "https://e.es/a\0".encode("utf-16-le")), (fuera, b"\0")]
    ruta_enlace_txt = os.path.join(dir, "enlace.txt")
    with open(ruta_enlace_txt, "w", encoding="utf-8", newline="") as f:
        f.write("Mira la web")
    pi, v, e = abrir(ruta_enlace_txt)
    u32.SendMessageW(e, EM_SETSEL, 8, 11)
    with en_portapapeles(F):
        u32.SendMessageW(v, WM_COMMAND, PEGAR, 0)
    ok(texto(e) == "Mira la https://e.es/a", "txt: la dirección se pega tal cual")
    ok(cerrar_sin_guardar(pi, v), "txt con enlace: se cierra bien")
    F = copiar("<span>sin formato</span>", "solo texto")

    # Código de un editor, fórmulas de KaTeX y texto con tabuladores (L14).
    fuera = u32.RegisterClipboardFormatW("ExcludeClipboardContentFromMonitorProcessing")
    vscode = u32.RegisterClipboardFormatW("vscode-editor-data")

    def de_vscode(plano):
        html = "<div style=\"color: #cccccc;white-space: pre;\">%s</div>" % "".join("<div><span>%s</span></div>" % l for l in plano.split("\r\n"))
        return copiar(html, plano) + [(vscode, b'{"version":1,"isFromEmptySelection":false,"mode":"python"}\0')]

    def katex(tex, plano):
        html = ("<p>Vale <span class=\"katex\"><span class=\"katex-mathml\"><math><semantics><mrow><mi>x</mi></mrow>"
                "<annotation encoding=\"application/x-tex\">%s</annotation></semantics></math></span>"
                "<span class=\"katex-html\" aria-hidden=\"true\"><span class=\"mord\">x\u200b2</span></span></span> si x</p>" % tex)
        return copiar(html, plano)

    def solo_texto(t):
        return [(13, (t + "\0").encode("utf-16-le")), (fuera, b"\0")]

    doc = "Antes\r\n\r\n```\r\ncodigo\r\n```\r\n\r\nDespués"
    ruta_l14 = os.path.join(dir, "pegar-l14.md")
    with open(ruta_l14, "w", encoding="utf-8", newline="") as f:
        f.write(doc)
    pi, v, e = abrir(ruta_l14)
    for que, F, desde, orden, esperado in (
            ("código de varias líneas de VS Code, en su bloque con la lengua y en sus líneas", de_vscode("x = 1\r\ny = 2"), 5, PEGAR,
             "Antes\r\n\r\n```python\r\nx = 1\r\ny = 2\r\n```\r\n\r\n```\r\ncodigo"),
            ("una línea de VS Code, tal cual", de_vscode("x = 1"), 5, PEGAR, "Antesx = 1\r\n"),
            ("VS Code con «Pegar sin formato», tal cual", de_vscode("x = 1\r\ny = 2"), 5, PEGAR_TEXTO, "Antesx = 1\r\ny = 2\r\n"),
            ("VS Code dentro de un bloque de código, tal cual", de_vscode("x = 1\r\ny = 2"), 11, PEGAR, "```\r\nx = 1\r\ny = 2codigo"),
            ("una fórmula de KaTeX, en TeX", katex("x^2", "Vale x2 si x"), 5, PEGAR, "AntesVale $x^2$ si x\r\n"),
            ("texto con tabuladores, en tabla", solo_texto("Nombre\tEdad\r\nAna\t30"), 5, PEGAR,
             "Antes\r\n\r\n| Nombre | Edad |\r\n| --- | --- |\r\n| Ana | 30 |\r\n\r\n```"),
            ("código sangrado con tabuladores, tal cual", solo_texto("if x:\r\n\ty = 1"), 5, PEGAR, "Antesif x:\r\n\ty = 1\r\n")):
        u32.SendMessageW(e, EM_SETSEL, desde, desde)
        with en_portapapeles(F):
            u32.SendMessageW(v, WM_COMMAND, orden, 0)
        ok(esperado in texto(e), "md: %s (%r)" % (que, texto(e)[:90]))
        u32.SendMessageW(v, WM_COMMAND, DESHACER, 0)
        ok(texto(e) == doc, "md: y un «Deshacer» lo deja como estaba (%s)" % que)
    ok(cerrar_sin_guardar(pi, v), "md con L14: se cierra bien")
    ruta_l14_txt = os.path.join(dir, "pegar-l14.txt")
    with open(ruta_l14_txt, "w", encoding="utf-8", newline="") as f:
        f.write("Antes")
    pi, v, e = abrir(ruta_l14_txt)
    for que, F, esperado in (("VS Code", de_vscode("x = 1\r\ny = 2"), "Antesx = 1\r\ny = 2"),
                             ("tabuladores", solo_texto("Nombre\tEdad\r\nAna\t30"), "AntesNombre\tEdad\r\nAna\t30")):
        u32.SendMessageW(e, EM_SETSEL, 5, 5)
        with en_portapapeles(F):
            u32.SendMessageW(v, WM_COMMAND, PEGAR, 0)
        ok(texto(e) == esperado, "txt: %s, tal cual" % que)
        u32.SendMessageW(v, WM_COMMAND, DESHACER, 0)
    ok(cerrar_sin_guardar(pi, v), "txt con L14: se cierra bien")
    F = copiar("<span>sin formato</span>", "solo texto")

    # Lo seleccionado se sustituye entero aunque acabe en un salto (RichEdit se lo quedaría), y deshacer lo deja igual.
    for ext, doc, pegado in (("md", "uno\r\ndos\r\ntres", "uno\r\nsolo textotres"), ("txt", "uno\r\ndos\r\n", "uno\r\nsolo texto")):
        ruta_salto = os.path.join(dir, "pegar-salto." + ext)
        with open(ruta_salto, "w", encoding="utf-8", newline="") as f:
            f.write(doc)
        pi, v, e = abrir(ruta_salto)
        u32.SendMessageW(e, EM_SETSEL, 4, 8)   # «dos» y su salto
        with en_portapapeles(F):
            u32.SendMessageW(v, WM_COMMAND, PEGAR, 0)
        ok(texto(e) == pegado, "%s: pegar sobre una línea con su salto la sustituye entera" % ext)
        u32.SendMessageW(v, WM_COMMAND, DESHACER, 0)
        ok(texto(e) == doc, "%s: y un «Deshacer» la deja como estaba" % ext)
        ok(cerrar_sin_guardar(pi, v), "%s con salto: se cierra bien" % ext)

    # Con el portapapeles ocupado un momento por otro programa, copiar espera y copia igual.
    ruta_copia = os.path.join(dir, "copiar.txt")
    with open(ruta_copia, "w", encoding="utf-8", newline="") as f:
        f.write("copia esto")
    pi, v, e = abrir(ruta_copia)
    u32.SendMessageW(e, EM_SETSEL, 0, -1)
    abierto, soltar = threading.Event(), threading.Event()
    copiado = en_portapapeles(None).__enter__()

    def ocupar():
        u32.OpenClipboard(None)
        abierto.set()
        soltar.wait(2)
        u32.CloseClipboard()
    hilo = threading.Thread(target=ocupar)
    hilo.start(); abierto.wait(2)
    threading.Timer(0.04, soltar.set).start()
    u32.SendMessageW(e, 0x301, 0, 0)   # WM_COPY
    hilo.join()
    copiado.hecho()
    ok(texto_portapapeles() == "copia esto", "txt: copia aunque otro programa tenga el portapapeles un momento")
    copiado.__exit__()
    ok(cerrar_sin_guardar(pi, v), "txt copiar: se cierra bien")

    # Lo copiado de una web grande se convierte con el portapapeles ya cerrado: los demás programas no esperan.
    grande = "".join("<p>Párrafo %d con <b>negrita</b> y <a href=\"https://e.es/%d\">un enlace</a>.</p>" % (i, i) for i in range(40000))
    F = copiar(grande, "texto")
    pi, v, e = abrir(ruta)
    u32.SendMessageW(e, EM_SETSEL, 0, 0)
    pegando = en_portapapeles(F).__enter__()
    u32.PostMessageW(v, WM_COMMAND, PEGAR, 0)
    # Sin preguntarle nada a Bloc mientras tanto: la pregunta esperaría a que acabe de pegar.
    peor, desde, fin = 0.0, None, time.perf_counter() + 2
    while time.perf_counter() < fin:
        if u32.OpenClipboard(None):
            u32.CloseClipboard()
            if desde is not None:
                peor, desde = max(peor, time.perf_counter() - desde), None
        elif desde is None:
            desde = time.perf_counter()
        time.sleep(0.002)
    if desde is not None:
        peor = max(peor, time.perf_counter() - desde)
    pegando.__exit__()
    ok(len(texto(e)) > len(grande) // 4, "md: pega lo copiado de una web grande")
    ok(peor < 0.25, "md: mientras convierte una web grande, el portapapeles está libre (ocupado como mucho %.0f ms)" % (peor * 1000))
    ok(cerrar_sin_guardar(pi, v), "md web grande: se cierra bien")

    # M4 (1.4): archivos copiados en el Explorador. En un .md guardado, enlaces con la ruta relativa si están en su
    # carpeta o debajo; en uno sin guardar, con la completa; en un .txt o sin formato, las rutas, una por línea.
    sub = os.path.join(dir, "sub")
    os.makedirs(sub, exist_ok=True)
    uno, dos, foto = os.path.join(dir, "uno.txt"), os.path.join(sub, "dos (2).md"), os.path.join(dir, "foto.png")
    fuera_de = os.path.join(os.environ["SystemRoot"], "win.ini")
    ruta_md = os.path.join(dir, "archivos.md")
    with open(ruta_md, "w", encoding="utf-8", newline="") as f:
        f.write("Antes")
    pi, v, e = abrir(ruta_md)
    for que, rutas, esperado in (
            ("un archivo de su carpeta", [uno], "[uno.txt](uno.txt)"),
            ("uno de una subcarpeta, con espacios y paréntesis", [dos], "[dos (2).md](sub/dos%20%282%29.md)"),
            ("una imagen", [foto], "![foto.png](foto.png)"),
            ("una carpeta", [sub], "[sub](sub)"),
            ("uno de fuera, con la ruta completa", [fuera_de], "[win.ini](%s)" % fuera_de.replace("\\", "/")),
            ("varios, en su orden y uno por línea", [foto, uno, sub], "![foto.png](foto.png)\r\n[uno.txt](uno.txt)\r\n[sub](sub)")):
        u32.SendMessageW(e, EM_SETSEL, 0, 0)
        with en_portapapeles(archivos(*rutas)):
            u32.SendMessageW(v, WM_COMMAND, PEGAR, 0)
        ok(texto(e) == esperado + "Antes", "archivos, .md guardado: %s (%r)" % (que, texto(e)[:80]))
        u32.SendMessageW(v, WM_COMMAND, DESHACER, 0)
    u32.SendMessageW(e, EM_SETSEL, 0, 0)
    with en_portapapeles(archivos(uno, dos)):
        u32.SendMessageW(v, WM_COMMAND, PEGAR_TEXTO, 0)
    ok(texto(e) == "%s\r\n%sAntes" % (uno, dos), "archivos, .md con «Pegar sin formato»: las rutas completas")
    ok(cerrar_sin_guardar(pi, v), "archivos, .md: se cierra bien")
    ruta_txt2 = os.path.join(dir, "archivos.txt")
    with open(ruta_txt2, "w", encoding="utf-8", newline="") as f:
        f.write("Antes")
    pi, v, e = abrir(ruta_txt2)
    u32.SendMessageW(e, EM_SETSEL, 0, 0)
    with en_portapapeles(archivos(uno, sub)):
        u32.SendMessageW(v, WM_COMMAND, PEGAR, 0)
    ok(texto(e) == "%s\r\n%sAntes" % (uno, sub), "archivos, .txt: las rutas completas, una por línea")
    ok(cerrar_sin_guardar(pi, v), "archivos, .txt: se cierra bien")
    with open(INI, "w") as f:
        f.write("ajuste=1\nbarra=1\nmax=0\nzoom=100\nnoche=0\nnuevos_md=1\n")
    pi, v, e = abrir(None)
    with en_portapapeles(archivos(uno)):
        u32.SendMessageW(v, WM_COMMAND, PEGAR, 0)
    ok(texto(e) == "[uno.txt](%s)" % uno.replace(" ", "%20").replace("\\", "/"), "archivos, .md sin guardar: con la ruta completa (%r)" % texto(e)[:80])
    ok(cerrar_sin_guardar(pi, v), "archivos, .md sin guardar: se cierra bien")
    with open(INI, "w") as f:
        f.write("ajuste=1\nbarra=1\nmax=0\nzoom=100\nnoche=0\n")

    # M5 (1.4): una imagen sola en un .md guardado se guarda como PNG en su carpeta y se enlaza; en uno sin guardar
    # pide «Guardar como» (cancelado, nada); en un .txt, nada; en una carpeta sin permiso de escritura, nada.
    import re
    import subprocess
    import zlib

    def png_1x1():
        def trozo(tipo, datos):
            return len(datos).to_bytes(4, "big") + tipo + datos + zlib.crc32(tipo + datos).to_bytes(4, "big")
        return (b"\x89PNG\r\n\x1a\n" + trozo(b"IHDR", (1).to_bytes(4, "big") * 2 + bytes([8, 2, 0, 0, 0]))
                + trozo(b"IDAT", zlib.compress(b"\0\xff\0\0")) + trozo(b"IEND", b""))

    fuera_m5 = u32.RegisterClipboardFormatW("ExcludeClipboardContentFromMonitorProcessing")
    PNG = [(u32.RegisterClipboardFormatW("PNG"), png_1x1()), (fuera_m5, b"\0")]
    pixel = (40).to_bytes(4, "little") + (1).to_bytes(4, "little") * 2 + (1).to_bytes(2, "little") + (32).to_bytes(2, "little") + bytes(24) + b"\x00\x80\xff\x00"
    DIB = [(8, pixel), (fuera_m5, b"\0")]
    enlace = re.compile(r"!\[\]\((imagenes%20\d{4}-\d\d-\d\d%20\d\d-\d\d-\d\d(%20\d+)?\.png)\)")
    carpeta_m5 = os.path.join(dir, "m5")
    os.makedirs(carpeta_m5, exist_ok=True)
    ruta_md = os.path.join(carpeta_m5, "imagenes.md")
    with open(ruta_md, "w", encoding="utf-8", newline="") as f:
        f.write("Antes")

    def pngs(c):
        return sorted(n for n in os.listdir(c) if n.endswith(".png"))

    pi, v, e = abrir(ruta_md)
    for que, formatos in (("un PNG", PNG), ("solo un mapa de bits (CF_DIB)", DIB)):
        antes = pngs(carpeta_m5)
        u32.SendMessageW(e, EM_SETSEL, 0, 0)
        with en_portapapeles(formatos):
            u32.SendMessageW(v, WM_COMMAND, PEGAR, 0)
        nuevos = [n for n in pngs(carpeta_m5) if n not in antes]
        m = enlace.match(texto(e))
        ok(len(nuevos) == 1 and m is not None and texto(e).endswith("Antes"), "imagen, .md guardado, %s: el PNG en su carpeta y su enlace (%r)" % (que, texto(e)[:70]))
        if nuevos:
            with open(os.path.join(carpeta_m5, nuevos[0]), "rb") as f:
                ok(f.read(8) == b"\x89PNG\r\n\x1a\n", "imagen, .md guardado, %s: el archivo es un PNG" % que)
        u32.SendMessageW(v, WM_COMMAND, DESHACER, 0)
        ok(texto(e) == "Antes" and len(pngs(carpeta_m5)) == len(antes) + 1, "imagen, %s: deshacer quita el enlace y el PNG se queda" % que)
    antes = pngs(carpeta_m5)
    for _ in range(2):
        u32.SendMessageW(e, EM_SETSEL, 0, 0)
        with en_portapapeles(PNG):
            u32.SendMessageW(v, WM_COMMAND, PEGAR, 0)
    nuevos = [n for n in pngs(carpeta_m5) if n not in antes]
    ok(len(nuevos) == 2 and len(enlace.findall(texto(e))) == 2, "imagen: dos seguidas, dos archivos con nombres distintos (%s)" % nuevos)
    ok(not [n for n in os.listdir(carpeta_m5) if n.endswith(".bloc-tmp")], "imagen: no deja temporales")
    ok(cerrar_sin_guardar(pi, v), "imagen, .md: se cierra bien")

    ruta_txt3 = os.path.join(carpeta_m5, "imagen.txt")
    with open(ruta_txt3, "w", encoding="utf-8", newline="") as f:
        f.write("Antes")
    antes = pngs(carpeta_m5)
    pi, v, e = abrir(ruta_txt3)
    with en_portapapeles(PNG):
        u32.SendMessageW(v, WM_COMMAND, PEGAR, 0)
    ok(texto(e) == "Antes" and pngs(carpeta_m5) == antes, "imagen, .txt: no pega nada ni guarda nada")
    ok(cerrar_sin_guardar(pi, v), "imagen, .txt: se cierra bien")

    with open(INI, "w") as f:
        f.write("ajuste=1\nbarra=1\nmax=0\nzoom=100\nnoche=0\nnuevos_md=1\n")
    pi, v, e = abrir(None)
    with en_portapapeles(PNG):
        u32.PostMessageW(v, WM_COMMAND, PEGAR, 0)
        d = esperar(lambda: next((h for h in ventanas(pi.dwProcessId) if clase(h) == "#32770"), None), 5)
        if d:
            u32.PostMessageW(d, WM_COMMAND, 2, 0)   # IDCANCEL
        esperar(lambda: not u32.IsWindow(d), 3)
    ok(d is not None and texto(e) == "", "imagen, .md sin guardar: sale «Guardar como» y, cancelado, no pega nada")
    ok(cerrar_sin_guardar(pi, v), "imagen, .md sin guardar: se cierra bien")
    with open(INI, "w") as f:
        f.write("ajuste=1\nbarra=1\nmax=0\nzoom=100\nnoche=0\n")

    cerrada = os.path.join(dir, "m5-sin-permiso")
    os.makedirs(cerrada, exist_ok=True)
    ruta_cerrada = os.path.join(cerrada, "imagenes.md")
    with open(ruta_cerrada, "w", encoding="utf-8", newline="") as f:
        f.write("Antes")
    yo = os.environ["USERNAME"]
    subprocess.run(["icacls", cerrada, "/deny", "%s:(AD,WD)" % yo], check=True, capture_output=True)
    try:
        pi, v, e = abrir(ruta_cerrada)
        with en_portapapeles(PNG):
            u32.SendMessageW(v, WM_COMMAND, PEGAR, 0)
        ok(texto(e) == "Antes" and pngs(cerrada) == [], "imagen, carpeta sin permiso de escritura: no pega nada")
        ok(cerrar_sin_guardar(pi, v), "imagen, carpeta sin permiso: se cierra bien")
    finally:
        subprocess.run(["icacls", cerrada, "/remove:d", yo], check=True, capture_output=True)

    # Portapapeles vacío, solo con una imagen y con el texto diferido (el programa que copió lo da al pedirlo): pegar no
    # cuelga ni pega nada raro.
    fuera = u32.RegisterClipboardFormatW("ExcludeClipboardContentFromMonitorProcessing")
    pi, v, e = abrir(ruta)
    F = []
    u32.SendMessageW(e, EM_SETSEL, 0, 0)
    with en_portapapeles(F):
        u32.SendMessageW(v, WM_COMMAND, PEGAR, 0)
    ok(texto(e) == original, "md: con el portapapeles vacío, pegar no hace nada")
    dib = (40).to_bytes(4, "little") + (1).to_bytes(4, "little") * 2 + (1).to_bytes(2, "little") + (32).to_bytes(2, "little") + bytes(24) + bytes(4)
    F = [(8, dib), (fuera, b"\0")]   # CF_DIB de un píxel
    with en_portapapeles(F):
        u32.SendMessageW(v, WM_COMMAND, PEGAR_TEXTO, 0)
    ok(texto(e) == original, "md: con solo una imagen, «Pegar sin formato» no hace nada (pegarla la guarda: M5)")

    PROC = ctypes.WINFUNCTYPE(w.LPARAM, w.HWND, w.UINT, w.WPARAM, w.LPARAM)
    u32.DefWindowProcW.argtypes = [w.HWND, w.UINT, w.WPARAM, w.LPARAM]
    u32.DefWindowProcW.restype = w.LPARAM

    def proc(h, m, wp, lp):
        if m == 0x305 and wp == 13:   # WM_RENDERFORMAT del texto
            b = "diferido\0".encode("utf-16-le")
            mem = k32.GlobalAlloc(0x2, len(b))
            ctypes.memmove(k32.GlobalLock(mem), b, len(b))
            k32.GlobalUnlock(mem)
            u32.SetClipboardData(13, mem)
            return 0
        return u32.DefWindowProcW(h, m, wp, lp)
    procedimiento = PROC(proc)

    class WNDCLASS(ctypes.Structure):
        _fields_ = [("style", w.UINT), ("lpfnWndProc", PROC), ("cbClsExtra", ctypes.c_int), ("cbWndExtra", ctypes.c_int),
                    ("hInstance", w.HINSTANCE), ("hIcon", w.HICON), ("hCursor", ctypes.c_void_p), ("hbrBackground", w.HBRUSH),
                    ("lpszMenuName", w.LPCWSTR), ("lpszClassName", w.LPCWSTR)]
    u32.RegisterClassW(ctypes.byref(WNDCLASS(0, procedimiento, 0, 0, None, None, None, None, None, "PruebaDiferido")))
    u32.CreateWindowExW.restype = w.HWND
    u32.CreateWindowExW.argtypes = [w.DWORD, w.LPCWSTR, w.LPCWSTR, w.DWORD, ctypes.c_int, ctypes.c_int, ctypes.c_int, ctypes.c_int,
                                    w.HWND, w.HMENU, w.HINSTANCE, w.LPVOID]
    dueno = u32.CreateWindowExW(0, "PruebaDiferido", None, 0, 0, 0, 0, 0, w.HWND(-3), None, None, None)   # solo mensajes
    diferido = en_portapapeles(None).__enter__()
    esperar(lambda: u32.OpenClipboard(dueno), 2)
    u32.EmptyClipboard()
    u32.SetClipboardData(13, None)
    m = k32.GlobalAlloc(0x2, 1)
    u32.SetClipboardData(fuera, m)
    u32.CloseClipboard()
    diferido.hecho()
    u32.SendMessageW(v, WM_COMMAND, PEGAR, 0)   # mientras espera, este hilo atiende el WM_RENDERFORMAT
    diferido.__exit__()
    ok(texto(e).startswith("diferido"), "md: pega lo que el otro programa da al pedírselo (texto diferido)")
    ok(cerrar_sin_guardar(pi, v), "md portapapeles raro: se cierra bien")
    u32.DestroyWindow(dueno)


try:
    pruebas()
except Saltada:
    print("saltada: el portapapeles tiene algo que no se sabe reponer (una imagen, archivos)")
print("\nTodo bien." if not fallos else "\n%d fallos." % fallos)
sys.exit(fallos)
