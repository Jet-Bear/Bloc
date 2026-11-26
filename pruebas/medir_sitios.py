"""M0: lo que cuesta Bloc en los sitios de Windows donde está, con el Bloc instalado y su registro de verdad (como lo
pide el Explorador): la vista previa al elegir el primer archivo (Bloc sin arrancar) y al pasar de uno a otro, frente a
la vista de texto de Windows; y la orden «Editar con Bloc» del menú corto. Las ventanas van en una ventana escondida y
fuera de la pantalla: no se ve nada. Se pasa desde el panel Terminal. Uso: python medir_sitios.py <carpeta de trabajo>"""
import ctypes
import os
import statistics
import subprocess
import sys
import time
from ctypes import wintypes as w

dir = sys.argv[1]
u32, ole = ctypes.windll.user32, ctypes.windll.ole32
os.makedirs(dir, exist_ok=True)
for i in range(10):
    with open(os.path.join(dir, "nota-%d.txt" % i), "w", encoding="utf-8", newline="") as f:
        f.write("Nota %d\r\n\r\n" % i + "Una línea de texto corriente para la vista previa.\r\n" * 40)


class GUID(ctypes.Structure):
    _fields_ = [("a", ctypes.c_uint32), ("b", ctypes.c_uint16), ("c", ctypes.c_uint16), ("d", ctypes.c_ubyte * 8)]


def guid(s):
    g = GUID()
    ole.CLSIDFromString(ctypes.c_wchar_p(s), ctypes.byref(g))
    return g


CLSID_BLOC = guid("{ABEA560F-4304-4504-946A-22EBDA55A847}")
CLSID_WINDOWS = guid("{1531d583-8375-4d3f-b5fb-d23bbd169f22}")   # Windows TXT Previewer
CLSID_EDITAR = guid("{9CE57C74-B375-4377-B6F5-E23308B5B0D5}")
IID_INICIAR, IID_VISTA = guid("{b824b49d-22ac-4161-ac8a-9916e8fa3f7f}"), guid("{8895b1c6-b41f-4c1c-a562-0d564250836f}")
IID_ORDEN = guid("{a08ce4d0-fa25-44ab-b57c-c7b1c323e0b9}")
u32.CreateWindowExW.restype = w.HWND
u32.CreateWindowExW.argtypes = [w.DWORD, w.LPCWSTR, w.LPCWSTR, w.DWORD, ctypes.c_int, ctypes.c_int, ctypes.c_int, ctypes.c_int,
                                w.HWND, w.HANDLE, w.HANDLE, ctypes.c_void_p]
ctypes.windll.shlwapi.SHCreateStreamOnFileEx.argtypes = [w.LPCWSTR, w.DWORD, w.DWORD, w.BOOL, ctypes.c_void_p, ctypes.POINTER(ctypes.c_void_p)]


def metodo(p, n, *tipos):
    tabla = ctypes.cast(ctypes.cast(p, ctypes.POINTER(ctypes.c_void_p))[0], ctypes.POINTER(ctypes.c_void_p))
    return ctypes.WINFUNCTYPE(ctypes.c_long, ctypes.c_void_p, *tipos)(tabla[n])


def bombear(segundos):
    m, fin = w.MSG(), time.perf_counter() + segundos
    while time.perf_counter() < fin:
        while u32.PeekMessageW(ctypes.byref(m), None, 0, 0, 1):
            u32.TranslateMessage(ctypes.byref(m))
            u32.DispatchMessageW(ctypes.byref(m))
        time.sleep(0.002)


def vista(host, clsid, contexto, archivo):
    """Milisegundos desde pedir la vista hasta que DoPreview vuelve, como hace el Explorador al elegir un archivo."""
    iniciar, flujo, previa = ctypes.c_void_p(), ctypes.c_void_p(), ctypes.c_void_p()
    t = time.perf_counter()
    r = ole.CoCreateInstance(ctypes.byref(clsid), None, contexto, ctypes.byref(IID_INICIAR), ctypes.byref(iniciar))
    if r != 0:
        return None
    ctypes.windll.shlwapi.SHCreateStreamOnFileEx(os.path.join(dir, archivo), 0, 0, False, None, ctypes.byref(flujo))
    metodo(iniciar, 3, ctypes.c_void_p, w.DWORD)(iniciar, flujo, 0)
    metodo(iniciar, 0, ctypes.c_void_p, ctypes.c_void_p)(iniciar, ctypes.byref(IID_VISTA), ctypes.byref(previa))
    rect = w.RECT(0, 0, 600, 500)
    metodo(previa, 3, w.HWND, ctypes.c_void_p)(previa, host, ctypes.byref(rect))
    metodo(previa, 5)(previa)
    ms = (time.perf_counter() - t) * 1000
    bombear(0.3)
    metodo(previa, 6)(previa)
    for p in (previa, flujo, iniciar):
        if p:
            metodo(p, 2)(p)
    bombear(0.2)
    return ms


def bloc_vivos():
    s = subprocess.run(["tasklist", "/FI", "IMAGENAME eq Bloc.exe", "/FO", "CSV", "/NH"], check=True, capture_output=True, text=True,
                       encoding="mbcs").stdout
    return [x for x in s.splitlines() if "Bloc.exe" in x]


ole.CoInitializeEx(None, 2)
# Escondida (sin WS_VISIBLE) y fuera de la pantalla: sus hijas, las vistas, tampoco se ven.
host = u32.CreateWindowExW(0, "STATIC", "medir", 0x02000000, -3000, -3000, 600, 500, None, None, None, None)
antes = len(bloc_vivos())
for nombre, clsid, contexto in (("Bloc", CLSID_BLOC, 4), ("Windows", CLSID_WINDOWS, 1)):
    tiempos = [vista(host, clsid, contexto, "nota-%d.txt" % i) for i in range(10)]
    if None in tiempos:
        print("%-8s no se pudo pedir la vista" % nombre)
        continue
    print("%-8s primer archivo %6.1f ms; los 9 siguientes, mediana %6.1f ms y peor %6.1f ms" % (
        nombre, tiempos[0], statistics.median(tiempos[1:]), max(tiempos[1:])), flush=True)
print("procesos de Bloc antes %d, después de las vistas %d" % (antes, len(bloc_vivos())))

orden = ctypes.c_void_p()
t = time.perf_counter()
r = ole.CoCreateInstance(ctypes.byref(CLSID_EDITAR), None, 4, ctypes.byref(IID_ORDEN), ctypes.byref(orden))
ms = (time.perf_counter() - t) * 1000
if r == 0:
    titulo = ctypes.c_wchar_p()
    t = time.perf_counter()
    metodo(orden, 3, ctypes.c_void_p, ctypes.POINTER(ctypes.c_wchar_p))(orden, None, ctypes.byref(titulo))
    print("menú corto: Bloc -Menu arranca y da la orden en %.1f ms; su nombre, %.1f ms más" % (ms, (time.perf_counter() - t) * 1000))
    metodo(orden, 2)(orden)
else:
    print("menú corto: la orden no está registrada en este PC (0x%08X)" % (r & 0xFFFFFFFF))
u32.DestroyWindow(host)
