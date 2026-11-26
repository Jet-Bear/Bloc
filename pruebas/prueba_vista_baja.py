"""La vista previa con integridad baja, como la pone Windows (prevhost y Bloc -Embedding van así): enseña el archivo y lo
que apunta va a LocalLow, porque a %APPDATA% no puede escribir (M0, 1.3.2).

Hace de Explorador en un escritorio oculto: se relanza a sí misma con integridad baja (con LOCALAPPDATA y APPDATA de una
caja, para no tocar los del usuario), arranca allí «Bloc.exe -Embedding» y le pide la vista de un .txt. Se pasa desde el
panel Terminal (COM de verdad). Uso: python prueba_vista_baja.py <Bloc.exe> <carpeta de trabajo>"""
import ctypes
import os
import shutil
import subprocess
import sys
import time
from ctypes import wintypes as w

k32, u32, adv, ole = ctypes.windll.kernel32, ctypes.windll.user32, ctypes.windll.advapi32, ctypes.windll.ole32
ESC = "BlocVistaBaja"


class SI(ctypes.Structure):
    _fields_ = [("cb", w.DWORD), ("r", w.LPWSTR), ("lpDesktop", w.LPWSTR), ("t", w.LPWSTR)] + \
        [(n, w.DWORD) for n in "abcdefgh"] + [("s", w.WORD), ("c", w.WORD), ("p", ctypes.c_void_p), ("i", w.HANDLE), ("o", w.HANDLE), ("e", w.HANDLE)]


class PI(ctypes.Structure):
    _fields_ = [("hProcess", w.HANDLE), ("hThread", w.HANDLE), ("pid", w.DWORD), ("tid", w.DWORD)]


class GUID(ctypes.Structure):
    _fields_ = [("a", ctypes.c_uint32), ("b", ctypes.c_uint16), ("c", ctypes.c_uint16), ("d", ctypes.c_ubyte * 8)]


def guid(s):
    g = GUID()
    ole.CLSIDFromString(ctypes.c_wchar_p(s), ctypes.byref(g))
    return g


def metodo(p, n, *tipos):
    tabla = ctypes.cast(ctypes.cast(p, ctypes.POINTER(ctypes.c_void_p))[0], ctypes.POINTER(ctypes.c_void_p))
    return ctypes.WINFUNCTYPE(ctypes.c_long, ctypes.c_void_p, *tipos)(tabla[n])


def bombear(segundos):
    m, fin = w.MSG(), time.time() + segundos
    while time.time() < fin:
        while u32.PeekMessageW(ctypes.byref(m), None, 0, 0, 1):
            u32.TranslateMessage(ctypes.byref(m))
            u32.DispatchMessageW(ctypes.byref(m))
        time.sleep(0.01)


def hijo(exe, caja):
    """Ya con integridad baja: el Explorador de mentira. Devuelve 0 si la vista enseñó el archivo."""
    u32.CreateWindowExW.restype = w.HWND
    u32.CreateWindowExW.argtypes = [w.DWORD, w.LPCWSTR, w.LPCWSTR, w.DWORD, ctypes.c_int, ctypes.c_int, ctypes.c_int, ctypes.c_int,
                                    w.HWND, w.HANDLE, w.HANDLE, ctypes.c_void_p]
    ctypes.windll.shlwapi.SHCreateStreamOnFileEx.argtypes = [w.LPCWSTR, w.DWORD, w.DWORD, w.BOOL, ctypes.c_void_p, ctypes.POINTER(ctypes.c_void_p)]
    ole.CoInitializeEx(None, 2)
    si = SI(); si.cb = ctypes.sizeof(SI); si.lpDesktop = ESC; pi = PI()
    k32.CreateProcessW(None, ctypes.create_unicode_buffer('"%s" -Embedding' % exe), None, None, False, 0, None, None, ctypes.byref(si), ctypes.byref(pi))
    time.sleep(1.5)
    host = u32.CreateWindowExW(0, "STATIC", "vista", 0x10000000 | 0x02000000, 0, 0, 640, 400, None, None, None, None)
    iniciar, flujo, previa = ctypes.c_void_p(), ctypes.c_void_p(), ctypes.c_void_p()
    r = ole.CoCreateInstance(ctypes.byref(guid("{ABEA560F-4304-4504-946A-22EBDA55A847}")), None, 4,
                             ctypes.byref(guid("{b824b49d-22ac-4161-ac8a-9916e8fa3f7f}")), ctypes.byref(iniciar))
    hijos = []
    if r == 0:
        ctypes.windll.shlwapi.SHCreateStreamOnFileEx(os.path.join(caja, "docs", "nota.txt"), 0, 0, False, None, ctypes.byref(flujo))
        metodo(iniciar, 3, ctypes.c_void_p, w.DWORD)(iniciar, flujo, 0)
        metodo(iniciar, 0, ctypes.c_void_p, ctypes.c_void_p)(iniciar, ctypes.byref(guid("{8895b1c6-b41f-4c1c-a562-0d564250836f}")), ctypes.byref(previa))
        rect = w.RECT(0, 0, 640, 400)
        metodo(previa, 3, w.HWND, ctypes.c_void_p)(previa, host, ctypes.byref(rect))
        metodo(previa, 5)(previa)
        bombear(2.5)   # la sonda va a los 500 ms
        enum = ctypes.WINFUNCTYPE(w.BOOL, w.HWND, w.LPARAM)
        pid = w.DWORD()
        u32.EnumChildWindows(host, enum(lambda x, _: hijos.append(x) or True), 0)
        hijos = [x for x in hijos if u32.GetWindowThreadProcessId(x, ctypes.byref(pid)) and pid.value == pi.pid]
        metodo(previa, 6)(previa)
        for p in (previa, flujo, iniciar):
            if p:
                metodo(p, 2)(p)
    u32.DestroyWindow(host)
    k32.TerminateProcess(pi.hProcess, 0)
    k32.WaitForSingleObject(pi.hProcess, 5000)
    print("hijo: CoCreateInstance 0x%08X, ventanas de Bloc %d" % (r & 0xFFFFFFFF, len(hijos)), flush=True)
    return 0 if r == 0 and hijos else 1


def con_integridad_baja(orden, entorno):
    """Lanza `orden` con el mismo usuario pero integridad baja y espera a que acabe; devuelve su código."""
    adv.ConvertStringSidToSidW.argtypes = [w.LPCWSTR, ctypes.POINTER(ctypes.c_void_p)]
    k32.GetCurrentProcess.restype = w.HANDLE
    adv.OpenProcessToken.argtypes = [w.HANDLE, w.DWORD, ctypes.POINTER(w.HANDLE)]
    adv.DuplicateTokenEx.argtypes = [w.HANDLE, w.DWORD, ctypes.c_void_p, ctypes.c_int, ctypes.c_int, ctypes.POINTER(w.HANDLE)]
    adv.SetTokenInformation.argtypes = [w.HANDLE, ctypes.c_int, ctypes.c_void_p, w.DWORD]
    adv.GetLengthSid.argtypes = [ctypes.c_void_p]
    adv.CreateProcessAsUserW.argtypes = [w.HANDLE, w.LPCWSTR, w.LPWSTR, ctypes.c_void_p, ctypes.c_void_p, w.BOOL, w.DWORD, ctypes.c_void_p,
                                         w.LPCWSTR, ctypes.c_void_p, ctypes.c_void_p]
    k32.WaitForSingleObject.argtypes = [w.HANDLE, w.DWORD]
    k32.GetExitCodeProcess.argtypes = [w.HANDLE, ctypes.POINTER(w.DWORD)]
    yo, dup = w.HANDLE(), w.HANDLE()
    adv.OpenProcessToken(k32.GetCurrentProcess(),0x0001 | 0x0002 | 0x0008 | 0x0080 | 0x0100, ctypes.byref(yo))
    adv.DuplicateTokenEx(yo, 0x02000000, None, 2, 1, ctypes.byref(dup))   # MAXIMUM_ALLOWED, SecurityImpersonation, TokenPrimary
    sid = ctypes.c_void_p()
    adv.ConvertStringSidToSidW("S-1-16-4096", ctypes.byref(sid))

    class Etiqueta(ctypes.Structure):
        _fields_ = [("sid", ctypes.c_void_p), ("atributos", w.DWORD)]

    e = Etiqueta(sid, 0x20)   # SE_GROUP_INTEGRITY
    if not adv.SetTokenInformation(dup, 25, ctypes.byref(e), ctypes.sizeof(e) + adv.GetLengthSid(sid)):
        raise OSError("SetTokenInformation %d" % k32.GetLastError())
    bloque = "".join("%s=%s\0" % kv for kv in entorno.items()) + "\0"
    si = SI(); si.cb = ctypes.sizeof(SI); si.lpDesktop = ESC; pi = PI()
    if not adv.CreateProcessAsUserW(dup, None, ctypes.create_unicode_buffer(orden), None, None, False, 0x00000400, ctypes.create_unicode_buffer(bloque),
                                    None, ctypes.byref(si), ctypes.byref(pi)):   # CREATE_UNICODE_ENVIRONMENT
        raise OSError("CreateProcessAsUserW %d" % k32.GetLastError())
    k32.WaitForSingleObject(pi.hProcess, 60_000)
    codigo = w.DWORD()
    k32.GetExitCodeProcess(pi.hProcess, ctypes.byref(codigo))
    return codigo.value


def padre(exe, dir):
    caja = os.path.join(dir, "caja-vista-baja")
    shutil.rmtree(caja, ignore_errors=True)
    for d in ("AppData", "Local", "LocalLow", "docs"):
        os.makedirs(os.path.join(caja, d))
    # LocalLow y la carpeta del archivo, abiertas a la integridad baja, como en un perfil de verdad (el archivo solo se lee).
    for d in ("LocalLow",):
        subprocess.run(["icacls", os.path.join(caja, d), "/setintegritylevel", "(OI)(CI)low"], capture_output=True, check=True)
    with open(os.path.join(caja, "docs", "nota.txt"), "w", encoding="utf-8") as f:
        f.write("Una nota de prueba.\r\n")
    # El escritorio oculto, abierto a la integridad baja (si no, Bloc no podría poner allí sus ventanas).
    sd = ctypes.c_void_p()
    ctypes.windll.advapi32.ConvertStringSecurityDescriptorToSecurityDescriptorW(ctypes.c_wchar_p("D:(A;;GA;;;WD)S:(ML;;NW;;;LW)"), 1, ctypes.byref(sd), None)

    class SA(ctypes.Structure):
        _fields_ = [("n", w.DWORD), ("sd", ctypes.c_void_p), ("heredar", w.BOOL)]

    sa = SA(ctypes.sizeof(SA), sd, False)
    u32.CreateDesktopW.restype = w.HANDLE
    esc = u32.CreateDesktopW(ESC, None, None, 0, 0x10000000, ctypes.byref(sa))
    entorno = dict(os.environ, APPDATA=os.path.join(caja, "AppData"), LOCALAPPDATA=os.path.join(caja, "Local"),
                   BLOC_INI=os.path.join(caja, "LocalLow", "bloc.ini"), BLOC_NOCHE="0", PYTHONIOENCODING="utf-8")
    orden = '"%s" -I "%s" --hijo "%s" "%s"' % (sys.executable, os.path.abspath(__file__), exe, caja)
    codigo = con_integridad_baja(orden, entorno)
    fallos = 0

    def ok(bien, que):
        nonlocal fallos
        fallos += not bien
        print(("ok    " if bien else "FALLO ") + que, flush=True)

    ok(codigo == 0, "con integridad baja, la vista previa de Bloc enseña el archivo (código %d)" % codigo)
    bajo = os.path.join(caja, "LocalLow", "Bloc", "errores.log")
    texto = ""
    if os.path.exists(bajo):
        with open(bajo, encoding="utf-8", errors="replace") as f:
            texto = f.read()
    ok("Vista previa (prueba para editar en ella)" in texto, "y lo que apunta (la sonda) va a LocalLow\\Bloc\\errores.log")
    ok(not os.path.exists(os.path.join(caja, "AppData", "Bloc", "errores.log")), "a %APPDATA% no llega nada (no podría)")
    u32.CloseDesktop(esc)
    shutil.rmtree(caja, ignore_errors=True)
    print("\nTodo bien." if not fallos else "\n%d fallos." % fallos)
    return fallos


if sys.argv[1] == "--hijo":
    sys.exit(hijo(sys.argv[2], sys.argv[3]))
sys.exit(padre(sys.argv[1], sys.argv[2]))
