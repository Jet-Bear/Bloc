"""M0: lo que tarda cada cambio de tamaño de la vista previa (SetRect), como lo vive el Explorador: la llamada entera, hasta
que vuelve, con un .txt y un .md pequeños y de 1 MB. Con Bloc y, para comparar, con la vista de texto de Windows (dentro
de este proceso, como en prevhost). Hace de Explorador en un escritorio oculto y no cambia nada del usuario.
Se pasa desde el panel Terminal (COM de verdad). Uso: python medir_vista.py <Bloc.exe> <carpeta de trabajo> [veces]"""
import ctypes
import os
import shutil
import statistics
import sys
import threading
import time
from ctypes import wintypes as w

exe, dir = sys.argv[1:3]
VECES = int(sys.argv[3]) if len(sys.argv) > 3 else 40
k32, u32, ole = ctypes.windll.kernel32, ctypes.windll.user32, ctypes.windll.ole32
caja = os.path.join(dir, "caja-medir")
shutil.rmtree(caja, ignore_errors=True)
os.makedirs(os.path.join(caja, "AppData"))
os.environ.update(APPDATA=os.path.join(caja, "AppData"), BLOC_INI=os.path.join(caja, "bloc.ini"), BLOC_NOCHE="0")

PARRAFO = ("Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor incididunt ut labore et dolore "
           "magna aliqua. Ut enim ad minim veniam, quis nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo.\r\n")
MD = ("## Apartado\r\n\r\nUn párrafo con **negrita**, *cursiva* y `código` que sigue un buen rato para que haga falta "
      "partirlo en varias líneas al estrechar la ventana.\r\n\r\n- una viñeta\r\n- otra viñeta\r\n\r\n"
      "| columna | otra | la tercera |\r\n|---|---|---|\r\n| 1 | dos | tres |\r\n| 4 | cinco | seis |\r\n\r\n")


def escribir(nombre, trozo, tam):
    with open(os.path.join(caja, nombre), "w", encoding="utf-8", newline="") as f:
        f.write(trozo * max(1, tam // len(trozo.encode("utf-8"))))


escribir("pequeno.txt", PARRAFO, 2 << 10)
escribir("grande.txt", PARRAFO, 1 << 20)
escribir("pequeno.md", MD, 2 << 10)
escribir("grande.md", MD, 1 << 20)


class GUID(ctypes.Structure):
    _fields_ = [("a", ctypes.c_uint32), ("b", ctypes.c_uint16), ("c", ctypes.c_uint16), ("d", ctypes.c_ubyte * 8)]


def guid(s):
    g = GUID()
    ole.CLSIDFromString(ctypes.c_wchar_p(s), ctypes.byref(g))
    return g


CLSID_BLOC = guid("{ABEA560F-4304-4504-946A-22EBDA55A847}")
CLSID_WINDOWS = guid("{1531d583-8375-4d3f-b5fb-d23bbd169f22}")   # Windows TXT Previewer
IID_INICIAR, IID_VISTA = guid("{b824b49d-22ac-4161-ac8a-9916e8fa3f7f}"), guid("{8895b1c6-b41f-4c1c-a562-0d564250836f}")


class SI(ctypes.Structure):
    _fields_ = [("cb", w.DWORD), ("r", w.LPWSTR), ("lpDesktop", w.LPWSTR), ("t", w.LPWSTR)] + \
        [(n, w.DWORD) for n in "abcdefgh"] + [("s", w.WORD), ("c", w.WORD), ("p", ctypes.c_void_p), ("i", w.HANDLE), ("o", w.HANDLE), ("e", w.HANDLE)]


class PI(ctypes.Structure):
    _fields_ = [("hProcess", w.HANDLE), ("hThread", w.HANDLE), ("pid", w.DWORD), ("tid", w.DWORD)]


u32.CreateDesktopW.restype = w.HANDLE
u32.RedrawWindow.argtypes = [w.HWND, ctypes.c_void_p, ctypes.c_void_p, w.UINT]
ENUM = ctypes.WINFUNCTYPE(w.BOOL, w.HWND, w.LPARAM)
u32.EnumChildWindows.argtypes = [w.HWND, ENUM, w.LPARAM]
u32.CreateWindowExW.restype = w.HWND
u32.CreateWindowExW.argtypes = [w.DWORD, w.LPCWSTR, w.LPCWSTR, w.DWORD, ctypes.c_int, ctypes.c_int, ctypes.c_int, ctypes.c_int,
                                w.HWND, w.HANDLE, w.HANDLE, ctypes.c_void_p]
ctypes.windll.shlwapi.SHCreateStreamOnFileEx.argtypes = [w.LPCWSTR, w.DWORD, w.DWORD, w.BOOL, ctypes.c_void_p, ctypes.POINTER(ctypes.c_void_p)]
ESC = "BlocMedir"


def metodo(p, n, *tipos):
    tabla = ctypes.cast(ctypes.cast(p, ctypes.POINTER(ctypes.c_void_p))[0], ctypes.POINTER(ctypes.c_void_p))
    return ctypes.WINFUNCTYPE(ctypes.c_long, ctypes.c_void_p, *tipos)(tabla[n])


def bombear(segundos):
    m, fin = w.MSG(), time.perf_counter() + segundos
    while time.perf_counter() < fin:
        while u32.PeekMessageW(ctypes.byref(m), None, 0, 0, 1):
            u32.TranslateMessage(ctypes.byref(m))
            u32.DispatchMessageW(ctypes.byref(m))
        time.sleep(0.005)


def medir(clsid, contexto, archivo):
    """Milisegundos de cada SetRect (mediana y máximo) y de DoPreview, o None si no se pudo crear."""
    pi = None
    if contexto == 4:
        si = SI(); si.cb = ctypes.sizeof(SI); si.lpDesktop = ESC; pi = PI()
        k32.CreateProcessW(None, ctypes.create_unicode_buffer('"%s" -Embedding' % exe), None, None, False, 0, None, None, ctypes.byref(si), ctypes.byref(pi))
        time.sleep(1.5)
    host = u32.CreateWindowExW(0, "STATIC", "vista", 0x10000000 | 0x02000000, 0, 0, 900, 700, None, None, None, None)
    iniciar, flujo, previa = ctypes.c_void_p(), ctypes.c_void_p(), ctypes.c_void_p()
    r = ole.CoCreateInstance(ctypes.byref(clsid), None, contexto, ctypes.byref(IID_INICIAR), ctypes.byref(iniciar))
    salida = None
    if r == 0:
        ctypes.windll.shlwapi.SHCreateStreamOnFileEx(os.path.join(caja, archivo), 0, 0, False, None, ctypes.byref(flujo))
        metodo(iniciar, 3, ctypes.c_void_p, w.DWORD)(iniciar, flujo, 0)
        metodo(iniciar, 0, ctypes.c_void_p, ctypes.c_void_p)(iniciar, ctypes.byref(IID_VISTA), ctypes.byref(previa))
        rect = w.RECT(0, 0, 900, 700)
        metodo(previa, 3, w.HWND, ctypes.c_void_p)(previa, host, ctypes.byref(rect))
        t = time.perf_counter()
        metodo(previa, 5)(previa)
        ensenar = (time.perf_counter() - t) * 1000
        bombear(1.5)
        tiempos = []
        for k in range(VECES):
            ancho = 900 - (k % 20) * 20 if (k // 20) % 2 == 0 else 520 + (k % 20) * 20
            rect = w.RECT(0, 0, ancho, 700)
            t = time.perf_counter()
            metodo(previa, 4, ctypes.c_void_p)(previa, ctypes.byref(rect))
            # Lo que el Explorador tarda en poder seguir: la llamada y el repintado que deja pendiente, también el de la
            # ventana de Bloc (de otro proceso: RDW_UPDATENOW | RDW_ALLCHILDREN espera a que pinte).
            u32.RedrawWindow(host, None, None, 0x100 | 0x80)
            tiempos.append((time.perf_counter() - t) * 1000)
            bombear(0.016)
        # Y cuánto tarda la ventana de la vista en tener el último tamaño (si lo pone después de volver).
        hijos = []
        u32.EnumChildWindows(host, ENUM(lambda x, _: hijos.append(x) or True), 0)
        puesto = w.RECT()
        t = time.perf_counter()
        while hijos and time.perf_counter() - t < 3:
            u32.GetWindowRect(hijos[0], ctypes.byref(puesto))
            if puesto.right - puesto.left == rect.right:
                break
            bombear(0.002)
        salida = (statistics.median(tiempos), max(tiempos), ensenar, (time.perf_counter() - t) * 1000)
        metodo(previa, 6)(previa)
        for p in (previa, flujo, iniciar):
            if p:
                metodo(p, 2)(p)
    u32.DestroyWindow(host)
    if pi:
        k32.TerminateProcess(pi.hProcess, 0)
        k32.WaitForSingleObject(pi.hProcess, 5000)
    return r, salida


def trabajo():
    esc = u32.CreateDesktopW(ESC, None, None, 0, 0x10000000, None)
    u32.SetThreadDesktop(esc)
    ole.CoInitializeEx(None, 2)
    for nombre, clsid, contexto, archivos in (("Bloc", CLSID_BLOC, 4, ("pequeno.txt", "grande.txt", "pequeno.md", "grande.md")),
                                              ("Windows", CLSID_WINDOWS, 1, ("pequeno.txt", "grande.txt"))):
        for archivo in archivos:
            r, s = medir(clsid, contexto, archivo)
            if s:
                print("%-8s %-12s SetRect mediana %6.1f ms, máximo %6.1f ms; DoPreview %6.1f ms; último tamaño puesto %6.1f ms después" % (
                    nombre, archivo, s[0], s[1], s[2], s[3]), flush=True)
            else:
                print("%-8s %-12s no se pudo crear (0x%08X)" % (nombre, archivo, r & 0xFFFFFFFF), flush=True)


hilo = threading.Thread(target=trabajo)
hilo.start()
hilo.join()
registro = os.path.join(caja, "AppData", "Bloc", "errores.log")
if os.path.exists(registro):
    with open(registro, encoding="utf-8", errors="replace") as f:
        print("\nerrores.log de la caja:\n" + f.read().strip())
shutil.rmtree(caja, ignore_errors=True)
