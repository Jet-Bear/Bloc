"""Prueba el instalador de Bloc.exe en un escritorio oculto.
Uso: python prueba_instalar.py <Bloc.exe nuevo> <carpeta de trabajo> [--real]
Las ventanas de instalación, desinstalar y la actualización desde la carpeta de origen van a una carpeta y una clave de
registro de prueba (BLOC_PROGRAMAS, BLOC_REGISTRO y APPDATA propios): no tocan el Bloc instalado. Con --real, antes,
se instala de verdad (%LOCALAPPDATA%\\Programs\\Bloc), como en la entrega: al acabar queda instalado el Bloc.exe probado."""
import ctypes, ctypes.wintypes as w, os, pathlib, shutil, subprocess, sys, time, winreg

k32, u32, adv = ctypes.windll.kernel32, ctypes.windll.user32, ctypes.windll.advapi32
u32.CreateDesktopW.restype = w.HANDLE
u32.PostMessageW.argtypes = [w.HWND, w.UINT, w.WPARAM, w.LPARAM]
ENUM = ctypes.WINFUNCTYPE(w.BOOL, w.HWND, w.LPARAM)
u32.EnumDesktopWindows.argtypes = [w.HANDLE, ENUM, w.LPARAM]
u32.EnumChildWindows.argtypes = [w.HWND, ENUM, w.LPARAM]
adv.RegDeleteTreeW.argtypes = [w.HANDLE, w.LPCWSTR]
os.environ.pop("BLOC_SIN_INSTALAR", None)


class SI(ctypes.Structure):
    _fields_ = [("cb", w.DWORD), ("r", w.LPWSTR), ("lpDesktop", w.LPWSTR), ("t", w.LPWSTR)] + \
        [(n, w.DWORD) for n in "abcdefgh"] + [("s", w.WORD), ("c", w.WORD), ("p", ctypes.c_void_p),
                                               ("i", w.HANDLE), ("o", w.HANDLE), ("e", w.HANDLE)]


class PI(ctypes.Structure):
    _fields_ = [("hProcess", w.HANDLE), ("hThread", w.HANDLE), ("pid", w.DWORD), ("tid", w.DWORD)]


ESC = "BlocInstalar"
esc = u32.CreateDesktopW(ESC, None, None, 0, 0x10000000, None)
fallos = 0
# Una configuración propia, para no tocar la del usuario aunque esté usando Bloc a la vez.
os.environ["BLOC_INI"] = os.path.join(sys.argv[2], "bloc.ini")
k32.OpenProcess.restype = w.HANDLE
k32.QueryFullProcessImageNameW.argtypes = [w.HANDLE, w.DWORD, w.LPWSTR, ctypes.POINTER(w.DWORD)]
k32.CloseHandle.argtypes = [w.HANDLE]
k32.WaitForSingleObject.argtypes = [w.HANDLE, w.DWORD]
# Los botones del diálogo de instalación (instalar.rs) y los de Windows.
TDM_CLICK_BUTTON, BOTON_SI, BOTON_ABRIR, IDOK, IDCANCEL, IDCLOSE = 0x466, 100, 102, 1, 2, 8
# La página de elegir (eleccion.rs): las casillas de «Abrir con Bloc» y «Vista previa» de cada extensión, en el orden de
# FILAS, y la de iniciar con Windows. Archivo › «Archivos y vista previa…» la abre desde Bloc.
CASILLA, CASILLA_VISTA, CASILLA_INICIO, CASILLA_EDITAR, MENU_ELEGIR = 1000, 2000, 333, 335, 1008
EXTENSIONES = [".txt", ".md", ".markdown", ".log", ".ini", ".inf", ".csv", ".json", ".xml", ".ps1", ".psd1", ".psm1", ".scp", ".wtx"]
# Detrás de las de siempre, las que abre el Bloc de notas de Windows: .prueba desde el principio y .nueva, que sale después.
FILAS = EXTENSIONES + [".prueba", ".nueva"]


def ok(bien, que):
    global fallos
    print(("ok     " if bien else "FALLO  ") + que)
    fallos += 0 if bien else 1


def lanzar(exe, *args):
    si = SI(); si.cb = ctypes.sizeof(SI); si.lpDesktop = ESC; pi = PI()
    linea = " ".join('"%s"' % x for x in (exe,) + args)
    k32.CreateProcessW(None, ctypes.create_unicode_buffer(linea), None, None, False, 0, None, None,
                       ctypes.byref(si), ctypes.byref(pi))
    return pi


def acaba(pi, segundos=15):
    return k32.WaitForSingleObject(pi.hProcess, int(segundos * 1000)) == 0


def titulo(h):
    b = ctypes.create_unicode_buffer(512); u32.GetWindowTextW(h, b, 512); return b.value


def clase(h):
    b = ctypes.create_unicode_buffer(64); u32.GetClassNameW(h, b, 64); return b.value


def ventanas(que):
    res = []
    def cb(h, _):
        if u32.IsWindowVisible(h) and que(h):
            res.append(h)
        return True
    u32.EnumDesktopWindows(esc, ENUM(cb), 0)
    return res


def ventanas_bloc():
    return ventanas(lambda h: titulo(h).endswith(" - Bloc"))


def botones(h):
    """Los botones del diálogo de tareas son ventanas Button dentro de la suya: sus textos dicen qué página es."""
    res = []
    def cb(c, _):
        if clase(c) == "Button":
            res.append(titulo(c).replace("&", ""))
        return True
    u32.EnumChildWindows(h, ENUM(cb), 0)
    return res


def dialogo(boton, segundos=10):
    """El diálogo de instalación que tiene ese botón."""
    return esperar(lambda: next((h for h in ventanas(lambda h: clase(h) == "#32770" and titulo(h) == "Bloc")
                                 if boton in botones(h)), None), segundos)


def pulsar(h, boton):
    """El diálogo de tareas, por su mensaje; la página de elegir (un diálogo de Bloc, con casillas), como un clic."""
    u32.PostMessageW(h, 0x111 if u32.GetDlgItem(h, CASILLA) else TDM_CLICK_BUTTON, boton, 0)


def esperar(cond, segundos=10, paso=0.1):
    fin = time.time() + segundos
    while time.time() < fin:
        r = cond()
        if r:
            return r
        time.sleep(paso)
    return None


def cerrar_todas():
    for h in ventanas_bloc():
        u32.PostMessageW(h, 0x10, 0, 0)
    return esperar(lambda: not ventanas_bloc()) is not None or not ventanas_bloc()


def igual(a, b):
    with open(a, "rb") as x, open(b, "rb") as y:
        return x.read() == y.read()


DESTINOS = os.path.join(os.environ["APPDATA"], r"Microsoft\Windows\Recent\CustomDestinations")


def listas_puestas(desde):
    """Las listas de la barra de tareas que Windows guarda al poner Bloc «Nueva nota»."""
    res = []
    for n in os.listdir(DESTINOS) if os.path.isdir(DESTINOS) else []:
        p = os.path.join(DESTINOS, n)
        try:
            if os.path.getmtime(p) >= desde - 2:
                with open(p, "rb") as f:
                    if "Nueva nota".encode("utf-16-le") in f.read():
                        res.append(p)
        except (FileNotFoundError, PermissionError):   # el .temp que Windows escribe mientras pone una lista, ya renombrado o aún abierto
            pass
    return res


def en_uso():
    """Los .exe de los procesos abiertos: uno apartado sigue en uso mientras su Bloc siga abierto (el del usuario, por ejemplo)."""
    ids, n = (w.DWORD * 4096)(), w.DWORD()
    ctypes.windll.psapi.EnumProcesses(ids, ctypes.sizeof(ids), ctypes.byref(n))
    res = set()
    for pid in ids[:n.value // 4]:
        h = k32.OpenProcess(0x1000, False, pid)   # PROCESS_QUERY_LIMITED_INFORMATION
        if h:
            b, t = ctypes.create_unicode_buffer(1024), w.DWORD(1024)
            if k32.QueryFullProcessImageNameW(h, 0, b, ctypes.byref(t)):
                res.add(b.value.lower())
            k32.CloseHandle(h)
    return res


def apartados(destino):
    carpeta = os.path.dirname(destino)
    return [os.path.join(carpeta, n).lower() for n in os.listdir(carpeta) if n.startswith("Bloc.exe") and n.endswith(".viejo")]


nuevo, dir = sys.argv[1], sys.argv[2]
archivo = os.path.join(dir, "instalar.txt")
with open(archivo, "w", encoding="utf-8") as f:
    f.write("prueba del instalador")


def real():
    """La instalación de verdad, como hasta ahora: la misma versión que la instalada se pone sin preguntar."""
    destino = os.path.join(os.environ["LOCALAPPDATA"], "Programs", "Bloc", "Bloc.exe")
    lnk = os.path.join(os.environ["APPDATA"], r"Microsoft\Windows\Start Menu\Programs", "Bloc.lnk")
    if os.path.exists(lnk):
        os.remove(lnk)
    abierto = None
    inicio = time.time()
    distinto = not os.path.exists(destino) or not igual(nuevo, destino)
    if os.path.exists(destino):
        abierto = lanzar(destino)   # el Bloc instalado, abierto mientras se actualiza
        esperar(lambda: ventanas_bloc())
    pi = lanzar(nuevo, archivo)
    pi2 = lanzar(nuevo, archivo) if distinto else None   # dos clics seguidos: dos instalaciones a la vez
    # Si es otra versión, pregunta: se acepta.
    for _ in (pi, pi2) if pi2 else (pi,):
        h = dialogo("Instalar", 3) or dialogo("Actualizar", 1)
        if h:
            pulsar(h, BOTON_SI)
    ok(acaba(pi), "el instalador acaba solo")
    if pi2:   # si el segundo no pudiera instalar, avisaría y seguiría abierto desde donde está
        ok(acaba(pi2), "dos instalaciones a la vez acaban las dos sin error")
    ok(esperar(lambda: [h for h in ventanas_bloc() if titulo(h) == "instalar.txt - Bloc"]) is not None,
       "abre el archivo con la copia instalada")
    ok(igual(nuevo, destino), "la copia instalada es la nueva")
    ok(not [n for n in os.listdir(os.path.dirname(destino)) if n.endswith(".nuevo")], "no deja la copia temporal")
    if distinto:
        ok(esperar(lambda: listas_puestas(inicio), 15) is not None, "pone «Nueva nota» en la barra de tareas")
    if distinto and abierto:
        ok(apartados(destino), "con Bloc abierto, el viejo se aparta en vez de fallar")
    with winreg.OpenKey(winreg.HKEY_CURRENT_USER, r"Software\Classes\Applications\Bloc.exe\shell\open\command") as k:
        ok(winreg.QueryValue(k, None) == '"%s" "%%1"' % destino, "registrado en «Abrir con»")
    with winreg.OpenKey(winreg.HKEY_CURRENT_USER, r"Software\RegisteredApplications") as k:
        ok(winreg.QueryValueEx(k, "Bloc")[0] == r"Software\Bloc\Capabilities", "en «Aplicaciones predeterminadas»")
    with winreg.OpenKey(winreg.HKEY_CURRENT_USER, r"Software\Bloc\Capabilities\FileAssociations") as k:
        ok(winreg.QueryValueEx(k, ".ps1")[0] == "Bloc.Texto" and winreg.QueryValueEx(k, ".md")[0] == "Bloc.Markdown", "con sus tipos de archivo")
    with winreg.OpenKey(winreg.HKEY_CURRENT_USER, r"Software\Classes\Bloc.Texto\shell\open\command") as k:
        ok(winreg.QueryValue(k, None) == '"%s" "%%1"' % destino, "el tipo de archivo abre el instalado")
    ok(os.path.exists(lnk), "acceso directo en el menú Inicio")
    ok(cerrar_todas(), "se cierran las ventanas")
    if abierto:
        acaba(abierto, 5)
    lanzar(destino)
    esperar(lambda: ventanas_bloc())
    # Lo de menos de 10 minutos se deja a propósito (podría ser una instalación a medias): con las dos instalaciones a la
    # vez, la segunda puede apartar la copia que acaba de poner la primera.
    quedan = [a for a in apartados(destino) if a not in en_uso() and time.time() - os.path.getmtime(a) >= 600]
    ok(not quedan, "al abrir el instalado se borra lo apartado (menos el de un Bloc que siga abierto) %s" % quedan)
    cerrar_todas()
    antes = os.path.getmtime(destino)
    acaba(lanzar(nuevo))
    ok(esperar(lambda: ventanas_bloc()) is not None and os.path.getmtime(destino) == antes,
       "si ya está igual, no copia nada y abre el instalado")
    cerrar_todas()


if "--real" in sys.argv:
    real()

# --- Todo lo demás, en una carpeta y una clave de registro de prueba ---

RAIZ = r"Software\BlocPruebas"
caja = os.path.normpath(os.path.join(dir, "instalar-pruebas"))
prog = os.path.join(caja, "Programas", "Bloc")
destino = os.path.join(prog, "Bloc.exe")
os.environ.update(BLOC_PROGRAMAS=prog, BLOC_REGISTRO=RAIZ, APPDATA=os.path.join(caja, "AppData"))
lnk = os.path.join(caja, r"AppData\Microsoft\Windows\Start Menu\Programs\Bloc.lnk")
adv.RegDeleteTreeW(winreg.HKEY_CURRENT_USER, RAIZ)
shutil.rmtree(caja, ignore_errors=True)
os.makedirs(os.path.dirname(lnk))
raiz_bloc = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..")
sellar = os.path.join(raiz_bloc, "target", "release", "sellar.exe")
# La firma de las actualizaciones (L7): las copias con otra versión se vuelven a firmar, como al construir.
sys.path.insert(0, os.path.join(raiz_bloc, "privado", "firma"))
import firma  # noqa: E402


def version(v):
    """Una copia del Bloc.exe probado con otra versión en su recurso, como si fuera otra entrega."""
    d = os.path.join(caja, "fuera", v)
    os.makedirs(d, exist_ok=True)
    p = os.path.join(d, "Bloc.exe")
    shutil.copy(nuevo, p)
    subprocess.run([sellar, p, os.path.join(raiz_bloc, r"recursos\bloc.ico"), os.path.join(raiz_bloc, r"recursos\bloc.manifest"), v], check=True)
    # El antivirus mira el .exe recién escrito y a veces lo tiene abierto un momento.
    for intento in range(20):
        try:
            firma.firmar(p)
            break
        except PermissionError:
            if intento == 19:
                raise
            time.sleep(0.5)
    return p


v1, v2, v3, v4 = (version(v) for v in ("0.1.0", "0.2.0", "0.3.0", "0.4.0"))


def registro():
    """Todos los valores bajo la clave de prueba."""
    res = {}
    def leer(ruta):
        try:
            k = winreg.OpenKey(winreg.HKEY_CURRENT_USER, ruta)
        except OSError:
            return
        with k:
            n_claves, n_valores, _ = winreg.QueryInfoKey(k)
            for i in range(n_valores):
                n, v, t = winreg.EnumValue(k, i)
                res[(ruta[len(RAIZ) + 1:], n)] = v
            hijas = [winreg.EnumKey(k, i) for i in range(n_claves)]
        for h in hijas:
            leer(ruta + "\\" + h)
            if h.startswith(".") or "Bloc" in h:
                res[((ruta + "\\" + h)[len(RAIZ) + 1:], "(clave)")] = ""
    leer(RAIZ)
    return res


DES = r"Software\Microsoft\Windows\CurrentVersion\Uninstall\Bloc"


def valor(ruta, nombre):
    return registro().get((ruta, nombre))


def borrar_comprobado():
    try:
        with winreg.OpenKey(winreg.HKEY_CURRENT_USER, RAIZ + r"\Software\Bloc", 0, winreg.KEY_SET_VALUE) as k:
            winreg.DeleteValue(k, "Comprobado")
    except OSError:
        pass


def poner_origen(ruta):
    with winreg.CreateKey(winreg.HKEY_CURRENT_USER, RAIZ + r"\Software\Bloc") as k:
        winreg.SetValueEx(k, "Origen", 0, winreg.REG_SZ, ruta)


FE = r"Software\Microsoft\Windows\CurrentVersion\Explorer\FileExts"
IID_VISTA = "{8895B1C6-B41F-4C1C-A562-0D564250836F}"
CLSID_TEXTO, CLSID_MD = "{ABEA560F-4304-4504-946A-22EBDA55A847}", "{59237A87-8A0C-4A53-A263-DCC05BFC7936}"
POWERTOYS_MD = "{60789D87-9C3C-44AF-B18C-3DE2C2820ED3}"
NOTAS = "AppXPruebaNotas"


def poner(ruta, nombre, valor):
    with winreg.CreateKey(winreg.HKEY_CURRENT_USER, RAIZ + "\\" + ruta) as k:
        winreg.SetValueEx(k, nombre, 0, winreg.REG_SZ, valor)


for ext, prog in ((".txt", NOTAS), (".log", NOTAS), (".csv", "Excel.CSV"), (".xml", r"Applications\excelcnv.exe"), (".xlsx", r"Applications\excelcnv.exe")):
    poner(r"%s\%s\UserChoice" % (FE, ext), "ProgId", prog)
for ext in (".md", ".json"):
    poner(r"%s\%s\UserChoiceLatest\ProgId" % (FE, ext), "ProgId", r"Applications\Bloc.exe")
poner(r"Software\Classes\%s\Application" % NOTAS, "ApplicationName", "@{Microsoft.WindowsNotepad_11.2607.14.0_x64__8wekyb3d8bbwe?ms-resource://x}")
poner(r"Software\Classes\.markdown\shellex\%s" % IID_VISTA, "", POWERTOYS_MD)
# PowerShell abre .ps1; .markdown es de otro programa, pero sale marcada igual; .prueba la abre el Bloc de notas por la
# orden de su tipo, y .otra, otro programa: no sale.
poner(r"%s\.ps1\UserChoice" % FE, "ProgId", "PruebaPowerShell")
for tipo, orden in (("PruebaPowerShell", "powershell.exe -File %1"), ("PruebaTypora", "typora.exe %1"), ("PruebaOtroTipo", "otro.exe %1")):
    poner(r"Software\Classes\%s\shell\open\command" % tipo, "", orden)
with winreg.CreateKey(winreg.HKEY_CURRENT_USER, RAIZ + r"\Software\Classes\PruebaNotasTipo\shell\open\command") as k:
    winreg.SetValueEx(k, "", 0, winreg.REG_EXPAND_SZ, r"%SystemRoot%\system32\NOTEPAD.EXE %1")
for ext, tipo in ((".markdown", "PruebaTypora"), (".prueba", "PruebaNotasTipo"), (".otra", "PruebaOtroTipo")):
    poner(r"Software\Classes\%s" % ext, "", tipo)
# Un tipo de guiones cuyo «Editar» abre el Bloc de notas (como .bat o .reg), y otro que no.
ORDEN_NOTAS = r'"%SystemRoot%\system32\NOTEPAD.EXE" "%1"'
poner(r"Software\Classes\.guion", "", "PruebaGuion")
poner(r"Software\Classes\PruebaGuion\shell\edit\command", "", ORDEN_NOTAS)
poner(r"Software\Classes\PruebaOtroTipo\shell\edit\command", "", "otro.exe /editar %1")
de_antes = registro()
# Marcadas de entrada: lo que abre el Bloc de notas, Bloc o nada, y .markdown.
DE_ENTRADA = [e for e in FILAS[:15] if e not in (".csv", ".xml", ".ps1")]


def vista_de(base):
    return valor(r"Software\Classes\%s\shellex\%s" % (base, IID_VISTA), "")


def elegido():
    return valor(r"Software\Bloc", "Abrir"), valor(r"Software\Bloc", "Vista"), valor(RUN, "Bloc")


RUN = r"Software\Microsoft\Windows\CurrentVersion\Run"


def casillas(h, abrir, vista, inicio, editar=True):
    u32.SendMessageW(u32.GetDlgItem(h, CASILLA_EDITAR), 0xF1, editar, 0)
    for i, e in enumerate(FILAS):
        u32.SendMessageW(u32.GetDlgItem(h, CASILLA + i), 0xF1, e in abrir, 0)   # BM_SETCHECK
        u32.SendMessageW(u32.GetDlgItem(h, CASILLA_VISTA + i), 0xF1, e in vista, 0)
    u32.SendMessageW(u32.GetDlgItem(h, CASILLA_INICIO), 0xF1, inicio, 0)


def marcadas(h, base):
    return [e for i, e in enumerate(FILAS) if u32.SendMessageW(u32.GetDlgItem(h, base + i), 0xF0, 0, 0) == 1]   # BM_GETCHECK


# No instalado: pregunta; Cancelar no deja nada.
pi = lanzar(v2)
h = dialogo("Instalar")
ok(h is not None, "sin instalar, pregunta «Instalar Bloc»")
if h:
    pulsar(h, IDCANCEL)
ok(acaba(pi) and not os.path.exists(destino) and registro() == de_antes, "Cancelar no instala nada")

# Instalar: la página del final, el registro y el acceso directo.
inicio = time.time()
pi = lanzar(v2)
pulsar(dialogo("Instalar"), BOTON_SI)
h = dialogo("Abrir Bloc", 15)
ok(h is not None and "Elegir Bloc como predeterminado…" in botones(h), "después: «Bloc está instalado», con Elegir predeterminado y Abrir Bloc")
if h:
    ok(marcadas(h, CASILLA) == DE_ENTRADA and marcadas(h, CASILLA_VISTA) == DE_ENTRADA and not u32.GetDlgItem(h, CASILLA + 15)
       and u32.SendMessageW(u32.GetDlgItem(h, CASILLA_INICIO), 0xF0, 0, 0) == 1 and u32.SendMessageW(u32.GetDlgItem(h, CASILLA_EDITAR), 0xF0, 0, 0) == 1,
       "marcadas de entrada en las dos columnas: las del Bloc de notas (con .prueba), Bloc o nada y .markdown; no las de Excel ni PowerShell, ni sale .otra")
ok(os.path.exists(destino) and igual(destino, v2), "instalado en su carpeta")
r = registro()
ok(r.get((r"Software\Classes\Applications\Bloc.exe\shell\open\command", "")) == '"%s" "%%1"' % destino, "registrado en «Abrir con»")
ok(r.get((DES, "DisplayVersion")) == "0.2.0" and r.get((DES, "UninstallString")) == '"%s" /desinstalar' % destino
   and r.get((DES, "NoModify")) == 1 and r.get((DES, "NoRepair")) == 1 and r.get((DES, "EstimatedSize"), 0) > 1000
   and r.get((DES, "DisplayName")) == "Bloc", "entrada en Aplicaciones instaladas")
ok(os.path.normcase(r.get((r"Software\Bloc", "Origen"), "")) == os.path.normcase(v2), "apunta de dónde vino")
ok(os.path.exists(lnk), "acceso directo en el menú Inicio")
if h:
    pulsar(h, BOTON_ABRIR)
ok(acaba(pi) and esperar(lambda: ventanas_bloc()) is not None, "«Abrir Bloc» abre el instalado")
r = registro()
ok(elegido() == (" ".join(DE_ENTRADA), " ".join(DE_ENTRADA), '"%s"' % destino), "guarda lo elegido y pone el inicio con Windows")
ok(r.get((r"Software\Classes\.csv\OpenWithList\Bloc.exe", "(clave)")) is None and r.get((r"Software\Classes\.txt\OpenWithList\Bloc.exe", "(clave)")) == ""
   and r.get((r"Software\Bloc\Capabilities\FileAssociations", ".csv")) is None and r.get((r"Software\Bloc\Capabilities\FileAssociations", ".txt")) == "Bloc.Texto"
   and r.get((r"Software\Classes\.prueba\OpenWithList\Bloc.exe", "(clave)")) == "" and r.get((r"Software\Bloc\Capabilities\FileAssociations", ".prueba")) == "Bloc.Texto",
   "en «Abrir con» y en Aplicaciones predeterminadas, solo las marcadas, también las del Bloc de notas")
ok(vista_de(NOTAS) == CLSID_TEXTO and vista_de(r"Applications\Bloc.exe") == CLSID_TEXTO and vista_de(".prueba") == CLSID_TEXTO,
   "vista previa bajo el tipo de archivo elegido (Bloc de notas, Bloc) y, sin elegido, bajo la extensión")
ok(vista_de("Excel.CSV") is None and vista_de(".xml") is None and vista_de("PruebaPowerShell") is None and vista_de(".ps1") is None,
   "ninguna en lo de Excel ni PowerShell")
ok(vista_de(".markdown") == CLSID_MD and valor(r"Software\Bloc\VistaPrevia", r"Software\Classes\.markdown\shellex\%s" % IID_VISTA) == POWERTOYS_MD,
   "en .markdown, la de Markdown, y apunta la de PowerToys que había")
ORDEN_BLOC = '"%s" "%%1"' % destino
ok(valor(r"Software\Classes\SystemFileAssociations\text\shell\Bloc.Editar\command", "") == ORDEN_BLOC
   and valor(r"Software\Classes\SystemFileAssociations\text\shell\Bloc.Editar", "MUIVerb")
   and valor(r"Software\Classes\SystemFileAssociations\.md\shell\Bloc.Editar\command", "") == ORDEN_BLOC
   and valor(r"Software\Classes\SystemFileAssociations\.csv\shell\Bloc.Editar\command", "") is None,
   "«Editar con Bloc» en los archivos de texto y en las extensiones marcadas")
ok(valor(r"Software\Classes\PruebaGuion\shell\edit\command", "") == ORDEN_BLOC and valor(r"Software\Bloc\Editar", r"Software\Classes\pruebaguion\shell\edit\command") == ORDEN_NOTAS
   and valor(r"Software\Classes\PruebaOtroTipo\shell\edit\command", "") == "otro.exe /editar %1",
   "el «Editar» que abría el Bloc de notas abre Bloc, y se apunta el de antes; el de otro programa, no")
ok(valor(r"Software\Classes\CLSID\%s\LocalServer32" % CLSID_TEXTO, "") == '"%s"' % destino
   and valor(r"Software\Microsoft\Windows\CurrentVersion\PreviewHandlers", CLSID_MD) == "Bloc (Markdown)", "las dos clases de la vista previa, con el instalado")
lista = esperar(lambda: listas_puestas(inicio), 15)
ok(lista is not None, "pone «Nueva nota» en la barra de tareas")
ok(cerrar_todas(), "se cierran las ventanas")
instalado = registro()

# Más antigua instalada, con el instalado abierto: pregunta y la aparta.
abierto = lanzar(destino)
esperar(lambda: ventanas_bloc())
pi = lanzar(v3)
h = dialogo("Actualizar")
ok(h is not None, "con una más antigua instalada, pregunta «Actualizar Bloc»")
if h:
    pulsar(h, BOTON_SI)
ok(acaba(pi) and igual(destino, v3), "se actualiza")
ok(esperar(lambda: len(ventanas_bloc()) >= 2) is not None, "y abre el nuevo")
ok(apartados(destino), "el abierto se aparta como .viejo")
ok(valor(DES, "DisplayVersion") == "0.3.0", "la entrada dice la versión nueva")
ok(cerrar_todas(), "se cierran las ventanas")
acaba(abierto, 5)

# La misma: nada, abre la instalada.
antes = os.path.getmtime(destino)
pi = lanzar(v3)
ok(acaba(pi) and esperar(lambda: ventanas_bloc()) is not None and os.path.getmtime(destino) == antes and dialogo("Instalar", 0.5) is None,
   "la misma versión abre la instalada sin preguntar ni copiar")
cerrar_todas()

# La misma, puesta sin apuntar de dónde vino ni la entrada (a mano, o desde otra carpeta antes): las apunta. Y una copia
# igual de otro sitio no cambia el origen.
with winreg.OpenKey(winreg.HKEY_CURRENT_USER, RAIZ + r"\Software\Bloc", 0, winreg.KEY_SET_VALUE) as k:
    winreg.DeleteValue(k, "Origen")
adv.RegDeleteTreeW(winreg.HKEY_CURRENT_USER, RAIZ + "\\" + DES)
ok(acaba(lanzar(v3)) and esperar(lambda: ventanas_bloc()) is not None, "la misma, sin origen ni entrada, abre la instalada")
def origen():
    return os.path.normcase(valor(r"Software\Bloc", "Origen") or "")


ok(esperar(lambda: origen() == os.path.normcase(v3) and valor(DES, "DisplayVersion") == "0.3.0", 3) is not None,
   "y apunta el origen y la entrada de Aplicaciones instaladas")
cerrar_todas()
otra = os.path.join(caja, "otra")
os.makedirs(otra, exist_ok=True)
shutil.copy(v3, otra)
ok(acaba(lanzar(os.path.join(otra, "Bloc.exe"))) and esperar(lambda: ventanas_bloc()) is not None and origen() == os.path.normcase(v3),
   "una copia igual de otro sitio no cambia el origen")
cerrar_todas()

# Más nueva instalada: «Ya tienes una versión más nueva», nunca encima.
pi = lanzar(v2)
h = dialogo("Abrir Bloc")
ok(h is not None and "Actualizar" not in botones(h) and "Instalar" not in botones(h), "con una más nueva instalada, avisa y no ofrece instalar")
if h:
    pulsar(h, IDCANCEL)
ok(acaba(pi) and igual(destino, v3) and not ventanas_bloc(), "Cancelar no abre nada ni toca la instalada")
pi = lanzar(v2)
pulsar(dialogo("Abrir Bloc"), BOTON_ABRIR)
ok(acaba(pi) and esperar(lambda: ventanas_bloc()) is not None and igual(destino, v3), "«Abrir Bloc» abre la instalada")
cerrar_todas()

# Con un archivo y la instalada más nueva: lo abre la instalada, sin preguntar.
pi = lanzar(v2, archivo)
ok(acaba(pi) and esperar(lambda: [h for h in ventanas_bloc() if titulo(h) == "instalar.txt - Bloc"]) is not None
   and igual(destino, v3), "con un archivo, lo abre la instalada sin preguntar")
cerrar_todas()


def desinstalar():
    pi = lanzar(destino, "/desinstalar")
    pulsar(dialogo("Desinstalar"), BOTON_SI)
    h = dialogo("Cerrar", 15)
    if h:
        pulsar(h, IDCLOSE)
    hecho = acaba(pi) and h is not None and esperar(lambda: not os.path.exists(prog), 15) is not None
    # cmd la borra cada 2 s hasta que puede: con un .exe aún sin soltar, la carpeta puede volver a verse entre medias.
    time.sleep(2.5)
    return hecho and not os.path.exists(prog)


# Desinstalar: pregunta; Cancelar no quita nada; Desinstalar lo quita todo.
pi = lanzar(destino, "/desinstalar")
h = dialogo("Desinstalar")
ok(h is not None, "desinstalar pregunta antes")
config = os.path.join(caja, "AppData", "Bloc", "bloc.ini")
os.makedirs(os.path.dirname(config), exist_ok=True)
open(config, "w").close()
if h:
    pulsar(h, IDCANCEL)
ok(acaba(pi) and os.path.exists(destino) and registro() != de_antes, "Cancelar no quita nada")
ok(desinstalar(), "desinstalar borra la carpeta del programa")
ok(registro() == de_antes, "y lo que puso en el registro, con la vista previa de PowerToys de vuelta %s" % sorted(set(registro().items()) ^ set(de_antes.items()))[:4])
ok(not os.path.exists(lnk), "y el acceso del menú Inicio")
ok(not any(os.path.exists(p) for p in lista or []), "y la lista de la barra de tareas")
ok(os.path.exists(config), "la configuración se queda")

# Volver a instalar deja el registro como la primera vez.
pi = lanzar(v2)
pulsar(dialogo("Instalar"), BOTON_SI)
h = dialogo("Abrir Bloc", 15)
if h:
    pulsar(h, IDCLOSE)
acaba(pi)
# La hora de la última búsqueda de versiones la pone la ventana que se abrió la primera vez.
dif = {x for x in set(registro().items()) ^ set(instalado.items()) if x[0][1] != "Comprobado"}
ok(not dif, "desinstalar y volver a instalar deja el registro igual %s" % sorted(dif)[:4])
ok(esperar(lambda: listas_puestas(inicio), 15) is not None, "cerrado sin abrir Bloc, la lista de la barra de tareas se pone igual")

# --- Lo elegido, desde Archivo en Bloc ---


def pagina_desde_bloc():
    v = esperar(lambda: ventanas_bloc(), 10)
    if not v:
        return None, None
    u32.PostMessageW(v[0], 0x111, MENU_ELEGIR, 0)
    return v[0], esperar(lambda: next((h for h in ventanas(lambda h: clase(h) == "#32770" and titulo(h) == "Bloc")
                                       if "Aceptar" in botones(h)), None), 10, 0.005)


abierto = lanzar(destino)
v, h = pagina_desde_bloc()
ok(h is not None, "Archivo › «Archivos y vista previa…» abre la misma página")
if h:
    casillas(h, [], [], False, False)
    pulsar(h, IDOK)
time.sleep(0.5)
r = registro()
ok(elegido() == ("", "", None), "todo desmarcado: se guarda y se quita el inicio con Windows")
ok(not [k for k in r if "Bloc.Editar" in k[0]] and valor(r"Software\Classes\PruebaGuion\shell\edit\command", "") == ORDEN_NOTAS,
   "sin «Editar con Bloc», y el «Editar» vuelve al Bloc de notas")
ok(not [k for k in r if k[0].endswith(r"OpenWithList\Bloc.exe")] and not [k for k in r if IID_VISTA in k[0] and k[0] != r"Software\Classes\.markdown\shellex\%s" % IID_VISTA]
   and vista_de(".markdown") == POWERTOYS_MD and r.get((r"Software\Classes\CLSID\%s" % CLSID_TEXTO, "")) is None,
   "sin «Abrir con» ni vista previa: cada vista previa vuelve a la que había y fuera las clases")
v, h = pagina_desde_bloc()
if h:
    pulsar(h, IDCANCEL)
time.sleep(0.5)
ok(elegido() == ("", "", None), "Cancelar no cambia nada")
v, h = pagina_desde_bloc()
if h:
    casillas(h, [".txt", ".md"], [".md", ".csv"], True)
    pulsar(h, IDOK)
time.sleep(0.5)
ok(elegido() == (".txt .md", ".md .csv", '"%s"' % destino) and vista_de(".md") == CLSID_MD and vista_de(r"Applications\Bloc.exe") is None
   and vista_de("Excel.CSV") == CLSID_TEXTO and vista_de(NOTAS) is None,
   "otra elección: solo lo marcado; .md bajo la extensión, porque su tipo lo usa también .json, sin marcar")
elegida = {k: x for k, x in registro().items() if k[1] != "Comprobado"}
cerrar_todas()
acaba(abierto, 5)

# Una actualización por el instalador no cambia lo elegido.
pi = lanzar(v3)
pulsar(dialogo("Actualizar"), BOTON_SI)
fin = acaba(pi)
ok(fin and igual(destino, v3), "se actualiza a la 3 (acaba %s, igual %s)" % (fin, igual(destino, v3)))
cerrar_todas()
dif = {x for x in set(registro().items()) ^ set(elegida.items()) if x[0][1] not in ("Comprobado", "DisplayVersion", "EstimatedSize", "Origen")}
ok(not dif, "actualizar no cambia nada de lo elegido %s" % sorted(dif)[:4])

# Una extensión que el Bloc de notas abre después: sale marcada en la página, y lo elegido antes no cambia.
poner(r"Software\Classes\.nueva", "", "PruebaNotasTipo")
de_antes.update((k, x) for k, x in registro().items() if k[0] == r"Software\Classes\.nueva")
abierto = lanzar(destino)
esperar(lambda: ventanas_bloc(), 10)
v, h = pagina_desde_bloc()
ok(h is not None and marcadas(h, CASILLA) == [".txt", ".md", ".nueva"] and marcadas(h, CASILLA_VISTA) == [".md", ".csv", ".nueva"],
   "una del Bloc de notas nueva sale marcada; lo demás, como se eligió (%s)" % ((marcadas(h, CASILLA), marcadas(h, CASILLA_VISTA)) if h else None,))
tiempos = []
for _ in range(3):
    if h:
        pulsar(h, IDCANCEL)
    esperar(lambda h=h: not u32.IsWindow(h), 5, 0.01)
    t = time.perf_counter()
    v, h = pagina_desde_bloc()
    tiempos.append(time.perf_counter() - t)
ok(sorted(tiempos)[1] < 0.2, "la página sale en menos de 200 ms (%s ms)" % ", ".join("%.0f" % (x * 1000) for x in tiempos))
if h:
    pulsar(h, IDCANCEL)
time.sleep(0.5)
ok(elegido() == (".txt .md", ".md .csv", '"%s"' % destino), "y con Cancelar no se pone")
cerrar_todas()
acaba(abierto, 5)

# Puesta al día desde la 1.1, que no guardaba nada: la primera vez sale la página, una vez; Cancelar lo deja como estaba.
with winreg.OpenKey(winreg.HKEY_CURRENT_USER, RAIZ + r"\Software\Bloc", 0, winreg.KEY_SET_VALUE) as k:
    winreg.DeleteValue(k, "Abrir")
    winreg.DeleteValue(k, "Vista")
abierto = lanzar(destino)
h = dialogo("Aceptar", 10)
ok(h is not None, "sin nada elegido, el instalado enseña la página al abrirse")
if h:
    pulsar(h, IDCANCEL)
time.sleep(0.5)
ok(elegido()[:2] == (" ".join(EXTENSIONES), ""), "Cancelar la deja como la 1.1: las 14 en «Abrir con», sin vista previa")
cerrar_todas()
acaba(abierto, 5)
abierto = lanzar(destino)
ok(esperar(lambda: ventanas_bloc(), 10) is not None and dialogo("Aceptar", 2) is None, "y no vuelve a salir")
cerrar_todas()
acaba(abierto, 5)
shutil.copy(v2, destino)   # la de antes y su origen, para lo que sigue
poner_origen(v2)

# --- La actualización desde la carpeta de origen (la instalada es la 2, que vino de v2) ---


def abrir_instalado():
    t = time.perf_counter()
    pi = lanzar(destino)
    esperar(lambda: ventanas_bloc(), 10, 0.005)
    return pi, time.perf_counter() - t


# Solo con la firma de Bloc: sin ella, o con un byte cambiado, no se pone y se apunta.
with open(v3, "rb") as f:
    bien = f.read()
with open(destino, "rb") as f:
    instalada = f.read()
for que, malo in (("sin firma", firma.sin_firma(bien)), ("con un byte cambiado", bien[:5000] + bytes([bien[5000] ^ 1]) + bien[5001:])):
    with open(v2, "wb") as f:
        f.write(malo)
    borrar_comprobado()
    abierto, _ = abrir_instalado()
    time.sleep(3)
    log = os.path.join(caja, "AppData", "Bloc", "errores.log")
    apuntado = os.path.exists(log) and "no lleva la firma de Bloc" in pathlib.Path(log).read_text(encoding="utf-8")
    with open(destino, "rb") as f:
        igual_que_antes = f.read() == instalada
    ok(igual_que_antes and not apartados(destino) and apuntado,
       "una versión nueva %s no se pone, y se apunta (igual %s, apartados %s, apuntado %s)" % (que, igual_que_antes, apartados(destino), apuntado))
    cerrar_todas()
    if os.path.exists(log):
        os.remove(log)
shutil.copy(v3, v2)   # en la carpeta de origen se pone una versión más nueva, bien firmada
borrar_comprobado()
abierto, _ = abrir_instalado()
ok(esperar(lambda: igual(destino, v3), 15) is not None, "un origen más nuevo se instala solo, con la ventana abierta")
ok(ventanas_bloc() and apartados(destino), "la ventana sigue abierta con el viejo, apartado")
ok(valor(DES, "DisplayVersion") == "0.3.0", "la entrada dice la versión nueva")
otra, _ = abrir_instalado()
ok(esperar(lambda: len(ventanas_bloc()) >= 2) is not None, "la siguiente ventana abre el nuevo")
ok(cerrar_todas(), "se cierran las ventanas")

shutil.copy(v4, v2)
abierto, _ = abrir_instalado()
time.sleep(3)
ok(igual(destino, v3), "como mucho una vez por hora")
cerrar_todas()

shutil.copy(v1, v2)
borrar_comprobado()
abierto, _ = abrir_instalado()
time.sleep(3)
ok(igual(destino, v3), "un origen más antiguo no hace nada")
cerrar_todas()

# Un origen que no responde: abre igual de rápido y cierra sin esperarlo.
rapido, lento = [], []
for _ in range(3):
    pi, t = abrir_instalado()   # comprobado hace menos de una hora: no busca
    rapido.append(t); cerrar_todas(); acaba(pi, 5)
    poner_origen(r"\\bloc-no-existe.invalid\compartida\Bloc.exe")
    borrar_comprobado()
    pi, t = abrir_instalado()
    lento.append(t)
    time.sleep(0.3)
    t = time.perf_counter()
    cerrar_todas()
    ok(acaba(pi, 3), "cerrar no espera al origen que no responde (%.2f s)" % (time.perf_counter() - t))
rapido.sort(); lento.sort()
ok(lento[1] < rapido[1] + 0.15, "con el origen sin responder abre igual de rápido (%.3f s y %.3f s)" % (lento[1], rapido[1]))

ok(desinstalar() and registro() == de_antes, "al acabar, se desinstala")
log = os.path.join(caja, "AppData", "Bloc", "errores.log")
if os.path.exists(log):
    print(pathlib.Path(log).read_text(encoding="utf-8"))
# Un Bloc de la prueba que siguiese abierto volvería a escribir en la clave de prueba después de borrarla.
ids, n = (w.DWORD * 4096)(), w.DWORD()
ctypes.windll.psapi.EnumProcesses(ids, ctypes.sizeof(ids), ctypes.byref(n))
for pid in ids[:n.value // 4]:
    h = k32.OpenProcess(0x1001, False, pid)   # PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_TERMINATE
    if h:
        b, t = ctypes.create_unicode_buffer(1024), w.DWORD(1024)
        if k32.QueryFullProcessImageNameW(h, 0, b, ctypes.byref(t)) and b.value.lower().startswith(caja.lower()):
            k32.TerminateProcess(h, 0)
        k32.CloseHandle(h)
adv.RegDeleteTreeW(winreg.HKEY_CURRENT_USER, RAIZ)
shutil.rmtree(caja, ignore_errors=True)
os.remove(archivo)
u32.CloseDesktop(esc)
print("\nTodo bien." if not fallos else "\n%d fallos." % fallos)
sys.exit(fallos)
