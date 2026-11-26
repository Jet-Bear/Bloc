"""«Editar con Bloc» del menú corto de Windows 11 (L3): la orden que da «Bloc.exe -Menu» (IExplorerCommand), pedida por
COM como la pide el Explorador cuando el paquete de Bloc está puesto. Sin el paquete no se puede ver en el menú: aquí se
arranca el servidor a mano, en un escritorio oculto y con registro y configuración propios.
Se pasa desde el panel Terminal (COM de verdad). Uso: python prueba_menu.py <Bloc.exe> <carpeta de trabajo>"""
import ctypes
import os
import shutil
import sys
import threading
import time
import winreg
from ctypes import wintypes as w

exe, dir = sys.argv[1:3]
k32, u32, ole, shell = ctypes.windll.kernel32, ctypes.windll.user32, ctypes.windll.ole32, ctypes.windll.shell32
adv = ctypes.windll.advapi32
adv.RegDeleteTreeW.argtypes = [w.HANDLE, w.LPCWSTR]
RAIZ = r"Software\BlocPruebasMenu"
caja = os.path.join(dir, "caja-menu")
shutil.rmtree(caja, ignore_errors=True)
os.makedirs(os.path.join(caja, "AppData"))
os.environ.update(APPDATA=os.path.join(caja, "AppData"), BLOC_INI=os.path.join(caja, "bloc.ini"), BLOC_REGISTRO=RAIZ, BLOC_SIN_INSTALAR="1")
adv.RegDeleteTreeW(winreg.HKEY_CURRENT_USER, RAIZ)


def poner(nombre, valor):
    with winreg.CreateKey(winreg.HKEY_CURRENT_USER, RAIZ + r"\Software\Bloc") as k:
        winreg.SetValueEx(k, nombre, 0, winreg.REG_SZ, valor)


poner("Abrir", ".txt .md")
poner("Editar", "1")
archivos = {}
for n in ("carta.txt", "datos.csv"):
    archivos[n] = os.path.join(caja, n)
    with open(archivos[n], "w", encoding="utf-8") as f:
        f.write("hola\n")


class GUID(ctypes.Structure):
    _fields_ = [("a", ctypes.c_uint32), ("b", ctypes.c_uint16), ("c", ctypes.c_uint16), ("d", ctypes.c_ubyte * 8)]


def guid(s):
    g = GUID()
    ole.CLSIDFromString(ctypes.c_wchar_p(s), ctypes.byref(g))
    return g


CLSID_EDITAR = guid("{9CE57C74-B375-4377-B6F5-E23308B5B0D5}")
IID_ORDEN, IID_ELEMENTO, IID_ELEMENTOS = (guid("{a08ce4d0-fa25-44ab-b57c-c7b1c323e0b9}"), guid("{43826d1e-e718-42ee-bc55-a1e261c37bfe}"),
                                          guid("{b63ea76d-1f85-456f-a19c-48159efa858b}"))


class SI(ctypes.Structure):
    _fields_ = [("cb", w.DWORD), ("r", w.LPWSTR), ("lpDesktop", w.LPWSTR), ("t", w.LPWSTR)] + \
        [(n, w.DWORD) for n in "abcdefgh"] + [("s", w.WORD), ("c", w.WORD), ("p", ctypes.c_void_p), ("i", w.HANDLE), ("o", w.HANDLE), ("e", w.HANDLE)]


class PI(ctypes.Structure):
    _fields_ = [("hProcess", w.HANDLE), ("hThread", w.HANDLE), ("pid", w.DWORD), ("tid", w.DWORD)]


ENUM = ctypes.WINFUNCTYPE(w.BOOL, w.HWND, w.LPARAM)
u32.EnumDesktopWindows.argtypes = [w.HANDLE, ENUM, w.LPARAM]
u32.CreateDesktopW.restype = w.HANDLE
u32.PostMessageW.argtypes = [w.HWND, w.UINT, w.WPARAM, w.LPARAM]
k32.WaitForSingleObject.argtypes = [w.HANDLE, w.DWORD]
shell.SHCreateItemFromParsingName.argtypes = [w.LPCWSTR, ctypes.c_void_p, ctypes.c_void_p, ctypes.POINTER(ctypes.c_void_p)]
shell.SHCreateShellItemArrayFromShellItem.argtypes = [ctypes.c_void_p, ctypes.c_void_p, ctypes.POINTER(ctypes.c_void_p)]
ole.CoTaskMemFree.argtypes = [ctypes.c_void_p]
ESC = "BlocMenu"
fallos = 0


def ok(bien, que):
    global fallos
    fallos += not bien
    print(("ok    " if bien else "FALLO ") + que, flush=True)


def metodo(p, n, *tipos):
    tabla = ctypes.cast(ctypes.cast(p, ctypes.POINTER(ctypes.c_void_p))[0], ctypes.POINTER(ctypes.c_void_p))
    return ctypes.WINFUNCTYPE(ctypes.c_long, ctypes.c_void_p, *tipos)(tabla[n])


def soltar(p):
    metodo(p, 2)(p)


def elementos(ruta):
    e, a = ctypes.c_void_p(), ctypes.c_void_p()
    shell.SHCreateItemFromParsingName(ruta, None, ctypes.byref(IID_ELEMENTO), ctypes.byref(e))
    shell.SHCreateShellItemArrayFromShellItem(e, ctypes.byref(IID_ELEMENTOS), ctypes.byref(a))
    soltar(e)
    return a


def texto(orden, n, arr=None):
    s = ctypes.c_void_p()
    r = metodo(orden, n, ctypes.c_void_p, ctypes.POINTER(ctypes.c_void_p))(orden, arr, ctypes.byref(s))
    if r != 0 or not s:
        return None
    v = ctypes.wstring_at(s)
    ole.CoTaskMemFree(s)
    return v


def estado(orden, ruta):
    arr, e = elementos(ruta), w.DWORD(99)
    metodo(orden, 7, ctypes.c_void_p, w.BOOL, ctypes.POINTER(w.DWORD))(orden, arr, True, ctypes.byref(e))
    soltar(arr)
    return e.value


def esperar(cond, s=10):
    fin = time.time() + s
    while time.time() < fin:
        r = cond()
        if r:
            return r
        time.sleep(0.05)
    return None


def ventana_de(esc, titulo):
    def buscar():
        r = []
        u32.EnumDesktopWindows(esc, ENUM(lambda h, _: r.append(h) or True), 0)
        for h in r:
            b = ctypes.create_unicode_buffer(256)
            u32.GetWindowTextW(h, b, 256)
            if u32.IsWindowVisible(h) and b.value == titulo:
                return h
        return None
    return esperar(buscar)


def trabajo():
    esc = u32.CreateDesktopW(ESC, None, None, 0, 0x10000000, None)
    u32.SetThreadDesktop(esc)
    ole.CoInitializeEx(None, 2)
    si = SI(); si.cb = ctypes.sizeof(SI); si.lpDesktop = ESC; pi = PI()
    k32.CreateProcessW(None, ctypes.create_unicode_buffer('"%s" -Menu -Embedding' % exe), None, None, False, 0, None, None, ctypes.byref(si), ctypes.byref(pi))
    time.sleep(1.5)
    orden = ctypes.c_void_p()
    r = ole.CoCreateInstance(ctypes.byref(CLSID_EDITAR), None, 4, ctypes.byref(IID_ORDEN), ctypes.byref(orden))   # CLSCTX_LOCAL_SERVER
    ok(r == 0, "«Bloc.exe -Menu» da la orden (0x%08X)" % (r & 0xFFFFFFFF))
    if r != 0:
        return
    titulo = texto(orden, 3)
    ok(titulo == "Editar con Bloc", "su título, «Editar con Bloc» (%r)" % titulo)
    icono = texto(orden, 4)
    ok(icono is not None and icono.lower() == os.path.abspath(exe).lower() + ",0", "su icono, el de Bloc (%r)" % icono)
    g = GUID()
    ok(metodo(orden, 6, ctypes.POINTER(GUID))(orden, ctypes.byref(g)) == 0 and bytes(g) == bytes(CLSID_EDITAR), "su nombre, la clase")
    ok(estado(orden, archivos["carta.txt"]) == 0, "se enseña en una extensión marcada (.txt)")
    ok(estado(orden, archivos["datos.csv"]) == 2, "se esconde en una sin marcar (.csv)")
    poner("Editar", "0")
    ok(estado(orden, archivos["carta.txt"]) == 2, "y en todas sin «Editar con Bloc» marcado en la página")
    poner("Editar", "1")
    arr = elementos(archivos["carta.txt"])
    r = metodo(orden, 8, ctypes.c_void_p, ctypes.c_void_p)(orden, arr, None)
    soltar(arr)
    v = ventana_de(esc, "carta.txt - Bloc")
    ok(r == 0 and v is not None, "al pulsarla, abre el archivo en Bloc")
    if v:
        u32.PostMessageW(v, 0x10, 0, 0)   # WM_CLOSE
    soltar(orden)
    ok(k32.WaitForSingleObject(pi.hProcess, 0) == 0x102, "el servidor sigue un rato para el siguiente clic derecho")
    ok(k32.WaitForSingleObject(pi.hProcess, 40000) == 0, "y acaba solo, sin nadie que lo pida")
    k32.TerminateProcess(pi.hProcess, 0)


hilo = threading.Thread(target=trabajo)
hilo.start()
hilo.join()
adv.RegDeleteTreeW(winreg.HKEY_CURRENT_USER, RAIZ)
shutil.rmtree(caja, ignore_errors=True)
print("\nTodo bien." if not fallos else "\n%d fallos." % fallos)
sys.exit(fallos)
