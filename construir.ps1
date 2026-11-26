# Compila Bloc.exe: pruebas, compilación y recursos (icono, manifiesto y versión).
# Uso: .\construir.ps1 [ruta del Bloc.exe que se quiere]   (por defecto, target\Bloc.exe)
param([string]$Salida = "$PSScriptRoot\target\Bloc.exe")
$ErrorActionPreference = 'Stop'
$env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"
Push-Location $PSScriptRoot
try {
    cargo test --release --bin bloc
    if ($LASTEXITCODE) { throw 'Fallan las pruebas' }
    cargo build --release
    if ($LASTEXITCODE) { throw 'No compila' }
    Copy-Item target\release\bloc.exe $Salida -Force
    & target\release\sellar.exe $Salida recursos\bloc.ico recursos\bloc.manifest
    if ($LASTEXITCODE) { throw 'No se han podido poner el icono y el manifiesto' }
    # La firma de las actualizaciones, la última: cualquier cambio después la rompería. La clave solo está en privado\.
    python privado\firma\firma.py $Salida
    if ($LASTEXITCODE) { throw 'No se ha podido firmar' }
    Write-Host "Hecho: $Salida"
} finally {
    Pop-Location
}
