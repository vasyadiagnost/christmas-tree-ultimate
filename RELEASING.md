# Сборка Windows x64

Понадобятся Rust для `x86_64-pc-windows-msvc`, rustfmt, инструменты MSVC,
Windows SDK и Python 3. Для обработки MP3 также нужен FFmpeg с MP3-декодером
и libmp3lame. Сборка 0.2.2 использовала MSVC 14.51.36231 и FFmpeg 9.0.2.
Выполняйте команды из корня репозитория в PowerShell.

## Аудио

Для локальной сборки подготовьте встроенную петлю бубенцов. Команда ниже
читает готовый установщик из Releases и восстанавливает ровно тот WAV,
который использован в 0.2.2, после проверки его SHA-256. EXE не запускается.

```powershell
python scripts/audio-loop-santa.py --from-release '.\christmas-tree-ultimate-setup.exe'
```

Если у вас есть оригинальный MP3, петлю можно воспроизвести через FFmpeg.
Ссылки на запись и автора находятся в
[windows/assets/audio/README.txt](windows/assets/audio/README.txt).

```powershell
python scripts/audio-loop-santa.py --source 'C:\path\to\sleigh-bells.mp3' --ffmpeg (Get-Command ffmpeg).Source
```

Скрипт проверяет SHA-256 исходника и создаёт `windows/assets/audio/santa-bells.wav`.
Для точного соответствия 0.2.2 нужен оригинал с SHA-256
`F555288D54F88BDC66689286E3FCA04BFC56F98D1ADA82ED0986AC0F53F27B49`.
Авторство и параметры цикла описаны в
[windows/assets/audio/README.txt](windows/assets/audio/README.txt).

## Приложение и установщик

```powershell
.\scripts\build-windows.ps1
.\scripts\build-windows.ps1 -Installer
```

Первая команда проверяет тесты и собирает `dist/christmas-tree-ultimate.exe`.
Вторая также собирает `dist/christmas-tree-ultimate-setup.exe`. При подготовке
установщика сценарий проверяет размер, SHA-256, версию и подпись встроенного
пакета Microsoft. При первом запуске сборки пакет скачивается по URL из
[манифеста](windows/assets/prerequisites/vc-redist.json).
Для повторения версии используйте инструмент MSVC, соответствующий минимальной
версии DLL, указанной в [проверке компонентов](windows/src/prerequisites.rs).
Условия распространения этого компонента описаны в
[документации Microsoft](https://learn.microsoft.com/en-us/cpp/windows/redistributing-visual-cpp-files).

Подготовленный установщик можно проверить без установки приложения:

```powershell
$setup = Start-Process .\dist\christmas-tree-ultimate-setup.exe -ArgumentList '--check-prerequisites' -Wait -PassThru
$setup.ExitCode
```

Код `0` означает, что компоненты доступны, `1` — что проверка не прошла.
Проверка не устанавливает программу и компоненты.

## Проверки исходников

```powershell
.\scripts\verify-sources.ps1
cargo test --locked --manifest-path core/Cargo.toml
cargo test --locked --features setup --lib --manifest-path windows/Cargo.toml -- --test-threads=1
```

Эти проверки можно запускать до подготовки бубенцов. GitHub Actions выполняет
их при коммитах и pull requests. Тесты приложения и полная сборка запускаются
после подготовки аудио. Публикация релизов выполняется вручную.
