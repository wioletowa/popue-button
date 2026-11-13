# popue-button

Маленькое окно с одной кнопкой: нажимаешь — звучит «popue», и рот открывается.
Рот открыт, пока кнопка зажата или пока играет звук; повторный клик перезапускает звук.

Написано на Rust ([egui/eframe](https://github.com/emilk/egui) + [rodio](https://github.com/RustAudio/rodio)),
картинки и звук встроены в бинарник. Работает на Windows, Linux и macOS.

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
