$ErrorActionPreference = 'Stop'
$repository = (Resolve-Path (Join-Path $PSScriptRoot '..')).ProviderPath
$source = Join-Path $repository 'host'
$wslSharePrefix = '\\wsl$\'
if ($repository.StartsWith($wslSharePrefix, [System.StringComparison]::OrdinalIgnoreCase)) {
    $source = Join-Path ('\\wsl.localhost\' + $repository.Substring($wslSharePrefix.Length)) 'host'
    if (-not (Test-Path -LiteralPath $source -PathType Container)) {
        throw "Windows에서 WSL source 경로를 열 수 없습니다: $source"
    }
}

$cmakeCommand = Get-Command cmake.exe -ErrorAction SilentlyContinue
if ($cmakeCommand) {
    $cmake = $cmakeCommand.Source
} else {
    $visualStudioRoot = Join-Path $env:ProgramFiles 'Microsoft Visual Studio'
    $cmake = Get-ChildItem -LiteralPath $visualStudioRoot -Filter cmake.exe -File -Recurse |
        Where-Object { $_.FullName -like '*Microsoft\CMake\CMake\bin\cmake.exe' } |
        Sort-Object FullName -Descending |
        Select-Object -First 1 -ExpandProperty FullName
}

if (-not $cmake) {
    throw 'Windows CMake를 찾을 수 없습니다. Visual Studio의 C++ CMake 도구를 설치하세요.'
}

# x64 게임용 server(와 x86 loader)는 x64 configure가, x86 게임용 server는 Win32
# configure가 만든다. 각 server 옆에는 같은 비트 수의 bwrapi.dll·bwrapi-inject.exe가
# 복사된다(먼저 cargo xtask build로 만든다).
foreach ($architecture in 'x64', 'Win32') {
    $build = Join-Path $env:LOCALAPPDATA "BWRAPI\build\bwrapi-bwapi-$architecture"

    $cache = Join-Path $build 'CMakeCache.txt'
    if (Test-Path -LiteralPath $cache -PathType Leaf) {
        $homePrefix = 'CMAKE_HOME_DIRECTORY:INTERNAL='
        $cachedHome = Get-Content -LiteralPath $cache |
            Where-Object { $_.StartsWith($homePrefix, [System.StringComparison]::Ordinal) } |
            Select-Object -First 1
        if ($cachedHome) {
            $cachedSource = $cachedHome.Substring($homePrefix.Length).TrimEnd('/')
            $requestedSource = $source.Replace('\', '/').TrimEnd('/')
            if (-not [string]::Equals(
                $cachedSource,
                $requestedSource,
                [System.StringComparison]::OrdinalIgnoreCase
            )) {
                Remove-Item -LiteralPath $cache -Force
            }
        }
    }

    & $cmake -S $source -B $build -A $architecture -DCMAKE_SUPPRESS_REGENERATION=ON
    if ($LASTEXITCODE -ne 0) {
        throw "CMake configure가 실패했습니다: $LASTEXITCODE"
    }

    & $cmake --build $build --config Release
    if ($LASTEXITCODE -ne 0) {
        throw "Windows 빌드가 실패했습니다: $LASTEXITCODE"
    }

    Write-Host "빌드 완료: $build"
}
