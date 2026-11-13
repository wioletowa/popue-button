# popue-button

Маленькое окно с одной кнопкой: нажимаешь — звучит «popue», и рот открывается.
Рот открыт, пока кнопка зажата или пока играет звук; повторный клик перезапускает звук.

Написано на Rust: окно — [winit](https://github.com/rust-windowing/winit), отрисовка на CPU через
[softbuffer](https://github.com/rust-windowing/softbuffer) (без GPU), звук — [cpal](https://github.com/RustAudio/cpal).
Картинки, звук и буквы заголовка `build.rs` заранее превращает в сжатые сырые данные и встраивает в бинарник,
поэтому декодеры PNG/MP3/шрифтов в программу не попадают. Бинарник ~2 МБ, в памяти ~15–20 МБ,
в простое процессор не используется. Работает на Windows, Linux и macOS.

## Скачать

Готовые сборки — во вкладке [Releases](https://github.com/wioletowa/popue-button/releases)
(или в артефактах последнего запуска [Actions](https://github.com/wioletowa/popue-button/actions)).

- **Windows** — `popue-button-windows-x86_64.exe`
- **Linux** — `popue-button-linux-x86_64.tar.gz`
- **macOS** — `popue-button-macos-universal.zip` (Popue.app, Intel и Apple Silicon).
  Приложение не подписано сертификатом Apple, поэтому при первом запуске:
  правый клик → «Открыть».

## Сборка из исходников

```sh
cargo run --release
```

На Linux нужны `pkg-config` и заголовки ALSA (`libasound2-dev` / `alsa-lib`).

Исходники картинок лежат в `assets/src/`; в сборку идут подготовленные `assets/closed.png`, `assets/open.png`
(400×400, лица выровнены) и `assets/icon.png`. Шрифт заголовка — DejaVu Sans Bold (`assets/fonts/`).
