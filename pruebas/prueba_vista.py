"""La vista previa del Explorador decide Markdown o texto por la extensión del archivo, no por la clase con la que se
pide (L4): un .md sale igual por la clase de texto (la que va bajo Applications\\Bloc.exe y Bloc.Texto) que por la de
Markdown (Bloc.Markdown), y un .json con el mismo contenido, como texto por las dos.

Hace de Explorador en un escritorio oculto: arranca «Bloc.exe -Embedding» allí (con su configuración y su tema, sin
tocar los del usuario), le pide la vista previa por COM y compara capturas. Se pasa desde el panel Terminal: la activación
COM no sale del contenedor de las herramientas. Uso: python prueba_vista.py <Bloc.exe> <carpeta de trabajo>"""
import ctypes
import os
import shutil
import sys
import threading
import time
from ctypes import wintypes as w

exe, dir = sys.argv[1:3]
k32, u32, g32, ole = ctypes.windll.kernel32, ctypes.windll.user32, ctypes.windll.gdi32, ctypes.windll.ole32
caja = os.path.join(dir, "caja-vista")
shutil.rmtree(caja, ignore_errors=True)
os.makedirs(os.path.join(caja, "AppData"))
os.environ.update(APPDATA=os.path.join(caja, "AppData"), BLOC_INI=os.path.join(caja, "bloc.ini"), BLOC_NOCHE="0")
TEXTO = "# Título\r\n\r\nUn párrafo con **negrita** y `código`.\r\n\r\n| a | b |\r\n|---|---|\r\n| 1 | 2 |\r\n"
for ext in ("md", "json"):
    with open(os.path.join(caja, "igual." + ext), "w", encoding="utf-8", newline="") as f:
        f.write(TEXTO)


class GUID(ctypes.Structure):
    _fields_ = [("a", ctypes.c_uint32), ("b", ctypes.c_uint16), ("c", ctypes.c_uint16), ("d", ctypes.c_ubyte * 8)]


def guid(s):
    g = GUID()
    ole.CLSIDFromString(ctypes.c_wchar_p(s), ctypes.byref(g))
    return g


CLSID_TEXTO, CLSID_MD = guid("{ABEA560F-4304-4504-946A-22EBDA55A847}"), guid("{59237A87-8A0C-4A53-A263-DCC05BFC7936}")
IID_INICIAR, IID_VISTA = guid("{b824b49d-22ac-4161-ac8a-9916e8fa3f7f}"), guid("{8895b1c6-b41f-4c1c-a562-0d564250836f}")


class SI(ctypes.Structure):
    _fields_ = [("cb", w.DWORD), ("r", w.LPWSTR), ("lpDesktop", w.LPWSTR), ("t", w.LPWSTR)] + \
        [(n, w.DWORD) for n in "abcdefgh"] + [("s", w.WORD), ("c", w.WORD), ("p", ctypes.c_void_p), ("i", w.HANDLE), ("o", w.HANDLE), ("e", w.HANDLE)]


class PI(ctypes.Structure):
    _fields_ = [("hProcess", w.HANDLE), ("hThread", w.HANDLE), ("pid", w.DWORD), ("tid", w.DWORD)]


ENUM = ctypes.WINFUNCTYPE(w.BOOL, w.HWND, w.LPARAM)
u32.EnumChildWindows.argtypes = [w.HWND, ENUM, w.LPARAM]
u32.CreateDesktopW.restype = w.HANDLE
u32.CreateWindowExW.restype = w.HWND
u32.CreateWindowExW.argtypes = [w.DWORD, w.LPCWSTR, w.LPCWSTR, w.DWORD, ctypes.c_int, ctypes.c_int, ctypes.c_int, ctypes.c_int,
                                w.HWND, w.HANDLE, w.HANDLE, ctypes.c_void_p]
u32.GetDC.restype = g32.CreateCompatibleDC.restype = w.HDC
u32.SendMessageW.argtypes, u32.SendMessageW.restype = [w.HWND, w.UINT, w.WPARAM, w.LPARAM], w.LPARAM
u32.GetDC.argtypes = [w.HWND]
u32.ReleaseDC.argtypes = [w.HWND, w.HDC]
u32.RedrawWindow.argtypes = [w.HWND, ctypes.c_void_p, ctypes.c_void_p, w.UINT]
g32.CreateCompatibleBitmap.restype = g32.SelectObject.restype = w.HANDLE
g32.CreateCompatibleDC.argtypes = [w.HDC]
g32.CreateCompatibleBitmap.argtypes = [w.HDC, ctypes.c_int, ctypes.c_int]
g32.SelectObject.argtypes = [w.HDC, w.HANDLE]
g32.BitBlt.argtypes = [w.HDC, ctypes.c_int, ctypes.c_int, ctypes.c_int, ctypes.c_int, w.HDC, ctypes.c_int, ctypes.c_int, w.DWORD]
g32.GetDIBits.argtypes = [w.HDC, w.HANDLE, w.UINT, w.UINT, ctypes.c_void_p, ctypes.c_void_p, w.UINT]
g32.DeleteObject.argtypes = [w.HANDLE]
g32.DeleteDC.argtypes = [w.HDC]
ctypes.windll.shlwapi.SHCreateStreamOnFileEx.argtypes = [w.LPCWSTR, w.DWORD, w.DWORD, w.BOOL, ctypes.c_void_p, ctypes.POINTER(ctypes.c_void_p)]
ESC, ANCHO, ALTO = "BlocVista", 640, 400
fallos = 0


def ok(bien, que):
    global fallos
    fallos += not bien
    print(("ok    " if bien else "FALLO ") + que, flush=True)


def metodo(p, n, *tipos):
    """El método n de la interfaz p (sus primeros argumentos, el propio p)."""
    tabla = ctypes.cast(ctypes.cast(p, ctypes.POINTER(ctypes.c_void_p))[0], ctypes.POINTER(ctypes.c_void_p))
    return ctypes.WINFUNCTYPE(ctypes.c_long, ctypes.c_void_p, *tipos)(tabla[n])


def bombear(segundos):
    m, fin = w.MSG(), time.time() + segundos
    while time.time() < fin:
        while u32.PeekMessageW(ctypes.byref(m), None, 0, 0, 1):
            u32.TranslateMessage(ctypes.byref(m)); u32.DispatchMessageW(ctypes.byref(m))
        time.sleep(0.01)


def foto(h):
    u32.RedrawWindow(h, None, None, 0x100 | 0x80)   # RDW_UPDATENOW | RDW_ALLCHILDREN
    dc = u32.GetDC(h)
    m = g32.CreateCompatibleDC(dc)
    b = g32.CreateCompatibleBitmap(dc, ANCHO, ALTO)
    viejo = g32.SelectObject(m, b)
    g32.BitBlt(m, 0, 0, ANCHO, ALTO, dc, 0, 0, 0x00CC0020)
    datos = ctypes.create_string_buffer(ANCHO * ALTO * 4)
    bi = (ctypes.c_uint32 * 10)(40, ANCHO, (-ALTO) & 0xFFFFFFFF, 1 | (32 << 16), 0, 0, 0, 0, 0, 0)
    g32.GetDIBits(m, b, 0, ALTO, datos, bi, 0)
    g32.SelectObject(m, viejo); g32.DeleteObject(b); g32.DeleteDC(m); u32.ReleaseDC(h, dc)
    return datos.raw


def vista(clsid, archivo):
    """La captura de la vista previa de `archivo` pedida por esa clase, y si la sirvió el Bloc arrancado aquí."""
    si = SI(); si.cb = ctypes.sizeof(SI); si.lpDesktop = ESC; pi = PI()
    k32.CreateProcessW(None, ctypes.create_unicode_buffer('"%s" -Embedding' % exe), None, None, False, 0, None, None, ctypes.byref(si), ctypes.byref(pi))
    time.sleep(1.5)   # que ofrezca sus clases antes de pedirlas: así no arranca el Bloc instalado
    host = u32.CreateWindowExW(0, "STATIC", "vista", 0x10000000 | 0x02000000, 0, 0, ANCHO, ALTO, None, None, None, None)   # WS_VISIBLE | WS_CLIPCHILDREN
    iniciar, flujo, previa = ctypes.c_void_p(), ctypes.c_void_p(), ctypes.c_void_p()
    r = ole.CoCreateInstance(ctypes.byref(clsid), None, 4, ctypes.byref(IID_INICIAR), ctypes.byref(iniciar))   # CLSCTX_LOCAL_SERVER
    datos, mio = None, False
    if r == 0:
        ctypes.windll.shlwapi.SHCreateStreamOnFileEx(os.path.join(caja, archivo), 0, 0, False, None, ctypes.byref(flujo))
        metodo(iniciar, 3, ctypes.c_void_p, w.DWORD)(iniciar, flujo, 0)
        metodo(iniciar, 0, ctypes.c_void_p, ctypes.c_void_p)(iniciar, ctypes.byref(IID_VISTA), ctypes.byref(previa))
        rect = w.RECT(0, 0, ANCHO, ALTO)
        metodo(previa, 3, w.HWND, ctypes.c_void_p)(previa, host, ctypes.byref(rect))
        metodo(previa, 5)(previa)
        bombear(1.5)
        hijos = []
        u32.EnumChildWindows(host, ENUM(lambda x, _: hijos.append(x) or True), 0)
        pid = w.DWORD()
        mio = any(u32.GetWindowThreadProcessId(x, ctypes.byref(pid)) and pid.value == pi.pid for x in hijos)
        datos = foto(host)
        metodo(previa, 6)(previa)   # Unload
        for p in (previa, flujo, iniciar):
            if p:
                metodo(p, 2)(p)
    u32.DestroyWindow(host)
    k32.TerminateProcess(pi.hProcess, 0)
    k32.WaitForSingleObject(pi.hProcess, 5000)
    return r, mio, datos


def tamano():
    """M0 (1.3.2): cambiar el tamaño no hace esperar al Explorador (cada SetRect vuelve enseguida, también con un .md de
    1 MB) y la vista acaba con el último tamaño."""
    trozo = "## Apartado\r\n\r\nUn párrafo con **negrita** y `código` que sigue un rato.\r\n\r\n| a | b |\r\n|---|---|\r\n| 1 | 2 |\r\n\r\n"
    with open(os.path.join(caja, "grande.md"), "w", encoding="utf-8", newline="") as f:
        f.write(trozo * ((1 << 20) // len(trozo.encode("utf-8"))))
    si = SI(); si.cb = ctypes.sizeof(SI); si.lpDesktop = ESC; pi = PI()
    k32.CreateProcessW(None, ctypes.create_unicode_buffer('"%s" -Embedding' % exe), None, None, False, 0, None, None, ctypes.byref(si), ctypes.byref(pi))
    time.sleep(1.5)
    host = u32.CreateWindowExW(0, "STATIC", "vista", 0x10000000 | 0x02000000, 0, 0, 900, 600, None, None, None, None)
    iniciar, flujo, previa = ctypes.c_void_p(), ctypes.c_void_p(), ctypes.c_void_p()
    ole.CoCreateInstance(ctypes.byref(CLSID_MD), None, 4, ctypes.byref(IID_INICIAR), ctypes.byref(iniciar))
    ctypes.windll.shlwapi.SHCreateStreamOnFileEx(os.path.join(caja, "grande.md"), 0, 0, False, None, ctypes.byref(flujo))
    metodo(iniciar, 3, ctypes.c_void_p, w.DWORD)(iniciar, flujo, 0)
    metodo(iniciar, 0, ctypes.c_void_p, ctypes.c_void_p)(iniciar, ctypes.byref(IID_VISTA), ctypes.byref(previa))
    rect = w.RECT(0, 0, 900, 600)
    metodo(previa, 3, w.HWND, ctypes.c_void_p)(previa, host, ctypes.byref(rect))
    metodo(previa, 5)(previa)
    bombear(2)
    peor = 0.0
    for k in range(20):
        rect = w.RECT(0, 0, 880 - k * 15, 600)
        t = time.perf_counter()
        metodo(previa, 4, ctypes.c_void_p)(previa, ctypes.byref(rect))
        peor = max(peor, (time.perf_counter() - t) * 1000)
        bombear(0.016)
    hijos = []
    u32.EnumChildWindows(host, ENUM(lambda x, _: hijos.append(x) or True), 0)
    puesto, fin = w.RECT(), time.time() + 3
    while hijos and time.time() < fin:
        u32.GetWindowRect(hijos[0], ctypes.byref(puesto))
        if puesto.right - puesto.left == rect.right:
            break
        bombear(0.01)
    ok(peor < 30, "con un .md de 1 MB, cada cambio de tamaño vuelve enseguida al Explorador (el peor, %.0f ms)" % peor)
    ok(bool(hijos) and puesto.right - puesto.left == rect.right, "y la vista acaba con el último tamaño")
    metodo(previa, 6)(previa)
    for p in (previa, flujo, iniciar):
        if p:
            metodo(p, 2)(p)
    u32.DestroyWindow(host)
    k32.TerminateProcess(pi.hProcess, 0)
    k32.WaitForSingleObject(pi.hProcess, 5000)


def texto_grande():
    """M0 (1.3.4): un .txt de casi 1 MB (el tope de la vista) sale enseguida (el principio) y el resto llega a trozos sin hacer esperar a los
    cambios de tamaño, hasta quedar entero."""
    lineas = "".join("Línea %d de un registro largo, con algo de texto para que ocupe.\r\n" % i for i in range(14000))
    lineas += "FIN DEL ARCHIVO"
    with open(os.path.join(caja, "grande.txt"), "w", encoding="utf-8", newline="") as f:
        f.write(lineas)
    si = SI(); si.cb = ctypes.sizeof(SI); si.lpDesktop = ESC; pi = PI()
    k32.CreateProcessW(None, ctypes.create_unicode_buffer('"%s" -Embedding' % exe), None, None, False, 0, None, None, ctypes.byref(si), ctypes.byref(pi))
    time.sleep(1.5)
    host = u32.CreateWindowExW(0, "STATIC", "vista", 0x10000000 | 0x02000000, 0, 0, 900, 600, None, None, None, None)
    iniciar, flujo, previa = ctypes.c_void_p(), ctypes.c_void_p(), ctypes.c_void_p()
    ole.CoCreateInstance(ctypes.byref(CLSID_TEXTO), None, 4, ctypes.byref(IID_INICIAR), ctypes.byref(iniciar))
    ctypes.windll.shlwapi.SHCreateStreamOnFileEx(os.path.join(caja, "grande.txt"), 0, 0, False, None, ctypes.byref(flujo))
    metodo(iniciar, 3, ctypes.c_void_p, w.DWORD)(iniciar, flujo, 0)
    metodo(iniciar, 0, ctypes.c_void_p, ctypes.c_void_p)(iniciar, ctypes.byref(IID_VISTA), ctypes.byref(previa))
    rect = w.RECT(0, 0, 900, 600)
    metodo(previa, 3, w.HWND, ctypes.c_void_p)(previa, host, ctypes.byref(rect))
    t = time.perf_counter()
    metodo(previa, 5)(previa)
    abrir = (time.perf_counter() - t) * 1000
    peor = 0.0
    for k in range(20):
        rect = w.RECT(0, 0, 880 - k * 15, 600)
        t = time.perf_counter()
        metodo(previa, 4, ctypes.c_void_p)(previa, ctypes.byref(rect))
        peor = max(peor, (time.perf_counter() - t) * 1000)
        bombear(0.016)
    def es_texto(x):
        b = ctypes.create_unicode_buffer(64)
        u32.GetClassNameW(x, b, 64)
        return "RICHEDIT" in b.value.upper()
    cajas = []
    u32.EnumChildWindows(host, ENUM(lambda x, _: (cajas.append(x) if es_texto(x) else None) or True), 0)
    leido, fin = "", time.time() + 5
    while cajas and time.time() < fin and not leido.endswith("FIN DEL ARCHIVO"):
        bombear(0.1)
        n = u32.SendMessageW(cajas[0], 0x0E, 0, 0)   # WM_GETTEXTLENGTH
        b = ctypes.create_unicode_buffer(n + 1)
        u32.SendMessageW(cajas[0], 0x0D, n + 1, ctypes.addressof(b))   # WM_GETTEXT
        leido = b.value
    ok(abrir < 80, "un .txt de 1 MB se enseña enseguida (%.0f ms)" % abrir)
    ok(peor < 30, "y mientras llega el resto, cada cambio de tamaño vuelve enseguida (el peor, %.0f ms)" % peor)
    ok(leido.replace("\r\n", "\r") == lineas.replace("\r\n", "\r"), "y acaba entero y en orden (%d de %d caracteres)" % (len(leido), len(lineas)))
    metodo(previa, 6)(previa)
    for p in (previa, flujo, iniciar):
        if p:
            metodo(p, 2)(p)
    u32.DestroyWindow(host)
    k32.TerminateProcess(pi.hProcess, 0)
    k32.WaitForSingleObject(pi.hProcess, 5000)


def trabajo():
    esc = u32.CreateDesktopW(ESC, None, None, 0, 0x10000000, None)
    u32.SetThreadDesktop(esc)
    ole.CoInitializeEx(None, 2)
    fotos = {}
    for nombre, clsid in (("texto", CLSID_TEXTO), ("md", CLSID_MD)):
        for archivo in ("igual.md", "igual.json"):
            r, mio, datos = vista(clsid, archivo)
            ok(r == 0 and mio and datos is not None, "%s por la clase de %s: la sirve este Bloc (0x%08X)" % (archivo, nombre, r & 0xFFFFFFFF))
            fotos[(nombre, archivo)] = datos
    md, json = fotos[("texto", "igual.md")], fotos[("texto", "igual.json")]
    ok(md is not None and md == fotos[("md", "igual.md")], "un .md, igual por la clase de texto que por la de Markdown")
    ok(json is not None and json == fotos[("md", "igual.json")], "un .json, igual por las dos clases")
    ok(md != json, "y el .md con formato, distinto del .json con el mismo texto")
    tamano()
    texto_grande()


hilo = threading.Thread(target=trabajo)
hilo.start()
hilo.join()
shutil.rmtree(caja, ignore_errors=True)
print("\nTodo bien." if not fallos else "\n%d fallos." % fallos)
sys.exit(fallos)
