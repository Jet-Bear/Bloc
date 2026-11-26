"""«Informar de un fallo» (L11), en un escritorio de Windows aparte e invisible.
Uso: python prueba_informes.py <Bloc.exe> <carpeta de trabajo>
Con la carpeta «Bloc informes» junto a la de origen, el informe aparece ahí con su nombre y sin rutas, nombres de
archivo ni texto del documento; sin ella, la dirección de GitHub sale bien formada (sin abrir el navegador); con la
carpeta de origen sin responder, se guarda en el perfil y se manda al abrir Bloc otra vez."""
import ctypes
import ctypes.wintypes as w
import os
import pathlib
import re
import shutil
import sys
import time
import urllib.parse
import winreg

k32, u32 = ctypes.windll.kernel32, ctypes.windll.user32
u32.CreateDesktopW.restype = w.HANDLE
u32.SendMessageW.argtypes = [w.HWND, w.UINT, w.WPARAM, w.LPARAM]
u32.SendMessageW.restype = w.LPARAM
u32.PostMessageW.argtypes = [w.HWND, w.UINT, w.WPARAM, w.LPARAM]
u32.GetDlgItem.argtypes = [w.HWND, ctypes.c_int]
u32.GetDlgItem.restype = w.HWND
ENUM = ctypes.WINFUNCTYPE(w.BOOL, w.HWND, w.LPARAM)
u32.EnumDesktopWindows.argtypes = [w.HANDLE, ENUM, w.LPARAM]
WM_SETTEXT, WM_GETTEXT, WM_GETTEXTLENGTH, WM_CLOSE, WM_COMMAND = 0x0C, 0x0D, 0x0E, 0x10, 0x111
INFORMAR, CAJA, IDOK = 1009, 11, 1


class SI(ctypes.Structure):
    _fields_ = [("cb", w.DWORD), ("lpReserved", w.LPWSTR), ("lpDesktop", w.LPWSTR),
                ("lpTitle", w.LPWSTR), ("dwX", w.DWORD), ("dwY", w.DWORD),
                ("dwXSize", w.DWORD), ("dwYSize", w.DWORD), ("dwXCountChars", w.DWORD),
                ("dwYCountChars", w.DWORD), ("dwFillAttribute", w.DWORD), ("dwFlags", w.DWORD),
                ("wShowWindow", w.WORD), ("cbReserved2", w.WORD), ("lpReserved2", ctypes.c_void_p),
                ("hStdInput", w.HANDLE), ("hStdOutput", w.HANDLE), ("hStdError", w.HANDLE)]


class PI(ctypes.Structure):
    _fields_ = [("hProcess", w.HANDLE), ("hThread", w.HANDLE), ("dwProcessId", w.DWORD), ("dwThreadId", w.DWORD)]


exe, caja = sys.argv[1], os.path.abspath(sys.argv[2])
shutil.rmtree(caja, ignore_errors=True)
os.makedirs(caja)
ESCRITORIO = "BlocPruebasInformes"
escritorio = u32.CreateDesktopW(ESCRITORIO, None, None, 0, 0x10000000, None)
REGISTRO = r"Software\BlocPruebasInformes"
os.environ["BLOC_SIN_INSTALAR"] = "1"
os.environ["BLOC_REGISTRO"] = REGISTRO
os.environ["APPDATA"] = caja
os.environ["BLOC_INI"] = os.path.join(caja, "bloc.ini")
os.environ.pop("BLOC_REPOSITORIO", None)
URL = os.environ["BLOC_INFORME_URL"] = os.path.join(caja, "direccion.txt")
fallos = 0

# Lo que nunca puede salir: nombres de clientes, rutas, nombres de archivo y el texto del documento.
SECRETOS = ["GARCIA", "Clientes", "factura 2024", "privado.md", "TEXTO SECRETO", os.environ["USERNAME"], caja]
os.makedirs(os.path.join(caja, "Bloc"))
with open(os.path.join(caja, "Bloc", "errores.log"), "w", encoding="utf-8") as f:
    f.write("No se ha podido guardar C:\\Clientes\\GARCIA LOPEZ\\factura 2024.pdf: acceso denegado\n"
            "No se abre \\\\servidor\\Clientes\\GARCIA\\notas.txt\n"
            "panicked at src\\ventana.rs:120:9: algo\n")
documento = os.path.join(caja, "privado.md")
with open(documento, "w", encoding="utf-8") as f:
    f.write("# TEXTO SECRETO\n\n| a | b |\n|---|---|\n| 1 | 2 |\n")


def ok(bien, que):
    global fallos
    print(("ok     " if bien else "FALLO  ") + que)
    fallos += 0 if bien else 1


def origen(ruta):
    with winreg.CreateKey(winreg.HKEY_CURRENT_USER, REGISTRO + r"\Software\Bloc") as k:
        winreg.SetValueEx(k, "Origen", 0, winreg.REG_SZ, ruta)


def texto(h):
    n = u32.SendMessageW(h, WM_GETTEXTLENGTH, 0, 0)
    b = ctypes.create_unicode_buffer(n + 1)
    u32.SendMessageW(h, WM_GETTEXT, n + 1, ctypes.addressof(b))
    return b.value


def clase(h):
    b = ctypes.create_unicode_buffer(256)
    u32.GetClassNameW(h, b, 256)
    return b.value


def ventanas(pid):
    res = []

    def cb(h, _):
        p = w.DWORD()
        u32.GetWindowThreadProcessId(h, ctypes.byref(p))
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


def lanzar():
    si = SI()
    si.cb = ctypes.sizeof(SI)
    si.lpDesktop = ESCRITORIO
    pi = PI()
    if not k32.CreateProcessW(None, ctypes.create_unicode_buffer('"%s" "%s"' % (exe, documento)), None, None,
                              False, 0, None, None, ctypes.byref(si), ctypes.byref(pi)):
        sys.exit("CreateProcess: %d" % k32.GetLastError())
    u32.WaitForInputIdle(pi.hProcess, 15000)
    v = esperar(lambda: next((h for h in ventanas(pi.dwProcessId) if texto(h).endswith("privado.md - Bloc")), None))
    return pi, v


def cerrar(pi, v):
    u32.PostMessageW(v, WM_CLOSE, 0, 0)
    if k32.WaitForSingleObject(pi.hProcess, 6000) != 0:
        k32.TerminateProcess(pi.hProcess, 1)


def informar(que):
    """Abre Bloc con el documento, pide el informe, escribe `que` y pulsa Enviar."""
    pi, v = lanzar()
    if not v:
        ok(False, "Bloc abre")
        return
    u32.PostMessageW(v, WM_COMMAND, INFORMAR, 0)
    d = esperar(lambda: next((h for h in ventanas(pi.dwProcessId) if clase(h) == "#32770"), None), 5)
    ok(d is not None, "sale el diálogo de informar")
    if d:
        escrito = ctypes.create_unicode_buffer(que)
        cuadro = u32.GetDlgItem(d, CAJA)
        u32.SendMessageW(cuadro, WM_SETTEXT, 0, ctypes.addressof(escrito))
        ok(texto(cuadro) == que, "el cuadro recibe lo escrito")
        u32.PostMessageW(d, WM_COMMAND, IDOK, u32.GetDlgItem(d, IDOK))
        ok(esperar(lambda: d not in ventanas(pi.dwProcessId), 3) is not None, "Enviar cierra el diálogo al momento")
    time.sleep(1)
    cerrar(pi, v)


def limpio(s, que):
    fuera = [x for x in SECRETOS if x.lower() in s.lower()]
    ok(not fuera, "%s: sin rutas, nombres ni texto del documento %s" % (que, fuera))


# 1. En una carpeta compartida: «Bloc informes» junto a la carpeta de Bloc.
compartida = os.path.join(caja, "compartida")
os.makedirs(os.path.join(compartida, "Bloc"))
os.makedirs(os.path.join(compartida, "Bloc informes"))
origen(os.path.join(compartida, "Bloc", "Bloc.exe"))
informar("Se cerró al pegar una tabla")
hechos = os.listdir(os.path.join(compartida, "Bloc informes"))
ok(len(hechos) == 1 and re.fullmatch(r"\d{4}-\d\d-\d\d \d\d-\d\d \S+ \d+\.\d+\.\d+\.md", hechos[0] if hechos else ""),
   "con la carpeta, un informe con su nombre (%s)" % hechos)
if hechos:
    s = pathlib.Path(compartida, "Bloc informes", hechos[0]).read_text(encoding="utf-8")
    limpio(s, "el informe")
    ok("Se cerró al pegar una tabla" in s and "## Datos técnicos" in s and "1 tablas" in s and "Documento: .md" in s,
       "lleva lo escrito y los datos técnicos")
    ok("<archivo>.pdf" in s and "<archivo>.txt" in s and "src\\ventana.rs:120" in s,
       "las rutas del registro, cambiadas por <archivo>; lo de Bloc, tal cual")
ok(not os.path.exists(URL), "con la carpeta, no se abre GitHub")

# 2. Sin la carpeta (cualquier otra instalación): la página de incidencia nueva de GitHub.
fuera = os.path.join(caja, "otro", "Bloc")
os.makedirs(fuera)
origen(os.path.join(fuera, "Bloc.exe"))
os.environ["BLOC_REPOSITORIO"] = "prueba/bloc"
informar("No guarda\r\ncon tildes: áéí & ?=#")
direccion = pathlib.Path(URL).read_text(encoding="utf-8") if os.path.exists(URL) else ""
u = urllib.parse.urlsplit(direccion)
q = urllib.parse.parse_qs(u.query)
ok(u.scheme == "https" and u.netloc == "github.com" and u.path == "/prueba/bloc/issues/new" and len(direccion) <= 7000,
   "sin la carpeta, la dirección de GitHub (%d caracteres)" % len(direccion))
ok(q.get("title") == ["No guarda"] and "con tildes: áéí & ?=#" in q.get("body", [""])[0],
   "título y texto bien codificados")
limpio(urllib.parse.unquote(direccion), "la dirección")
os.environ.pop("BLOC_REPOSITORIO")

# 3. La carpeta de origen no responde: se guarda en el perfil y se manda al abrir Bloc con la carpeta de vuelta.
caida = os.path.join(caja, "red", "Bloc")
origen(os.path.join(caida, "Bloc.exe"))
informar("Sin red")
guardados = os.path.join(caja, "Bloc", "informes")
ok(len(os.listdir(guardados)) == 1 if os.path.isdir(guardados) else False, "sin red, se guarda en el perfil")
os.makedirs(caida)
os.makedirs(os.path.join(caja, "red", "Bloc informes"))
pi, v = lanzar()
llegado = esperar(lambda: os.listdir(os.path.join(caja, "red", "Bloc informes")), 8)
ok(bool(llegado) and not os.listdir(guardados), "con la red de vuelta, se manda al abrir Bloc")
if v:
    cerrar(pi, v)



def borrar(ruta):
    with winreg.OpenKey(winreg.HKEY_CURRENT_USER, ruta) as k:
        hijas = [winreg.EnumKey(k, i) for i in range(winreg.QueryInfoKey(k)[0])]
    for h in hijas:
        borrar(ruta + "\\" + h)
    winreg.DeleteKey(winreg.HKEY_CURRENT_USER, ruta)


borrar(REGISTRO)
u32.CloseDesktop(escritorio)
print("Fallos: %d" % fallos)
sys.exit(1 if fallos else 0)
