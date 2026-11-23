# Two Windows in One

Разработчик [qqew666](https://github.com/qqew666).

Лаунчер на Rust для Windows 10 и 11 x64. Выберите открытое приложение или укажите файл `.exe`. Лаунчер запускает программу и может встроить окно нового процесса в рабочую область.

[Скачать для Windows x64](https://github.com/qqew666/wo-Windows-in-One/releases/latest) · [Сайт и ключ доступа](https://two-windows.sillypetunia8.chatgpt.site/)

## Запуск на ПК

1. Скачайте архив, распакуйте его и запустите `TwoWindowsInOne.exe`.
2. Пройдите CAPTCHA на сайте, скопируйте ключ и активируйте его. Смена ключа происходит в полночь по Москве.
3. Выберите программу в списке или укажите её файл `.exe`, затем нажмите **Открыть программу**. Для Chrome, Edge, Brave, Chromium, Vivaldi, Firefox, VS Code, VS Code Insiders и VSCodium лаунчер автоматически создаёт отдельные постоянные профили. Другие программы получают обычную команду запуска.
4. Если новое окно допускает встраивание, оно появится в рабочей области. **Отсоединить окно** возвращает его на рабочий стол. Если приложение переключает вас на прежнее окно или второму экземпляру нужна отдельная система, выберите Windows Sandbox.

Стабильные профили хранятся в `%LOCALAPPDATA%\TwoWindowsInOne\Profiles`. Программа использует обычные права текущего пользователя. Если Windows требует повышенных прав для выбранного файла, подтверждение запрашивает UAC.

## Запуск в Windows Sandbox

1. Включите Windows Sandbox в компонентах Windows. Нужны поддерживаемая редакция Pro, Enterprise или Education и аппаратная виртуализация. [Инструкция Microsoft](https://learn.microsoft.com/windows/security/application-security/application-isolation/windows-sandbox/windows-sandbox-install)
2. Выберите режим **Windows Sandbox** и файл программы или установщик `.exe` либо `.msi`.
3. Для самостоятельной программы или установщика выберите перенос одного файла. Если нужны соседние файлы, разрешите чтение показанной папки. Лаунчер копирует файлы внутрь гостевой системы и запускает их там.
4. Включите **Интернет в песочнице**, если программе нужен вход в аккаунт, загрузка компонентов или сохранение результата в облако. Данные внутри гостевой системы удаляются при закрытии.

Windows поддерживает одну гостевую песочницу за раз. Некоторым установленным программам нужны их установщики. Приложения Microsoft Store, программы с системными драйверами и ПО, которое запрещает работу в виртуальной машине, могут быть несовместимы с этим режимом.

## Сборка

Установите стабильный Rust и средства сборки Visual Studio с поддержкой C++. Из корня репозитория запустите проверку и сборку.

```powershell
cargo test --locked --manifest-path app/Cargo.toml --target x86_64-pc-windows-msvc
cargo build --locked --release --manifest-path app/Cargo.toml --target x86_64-pc-windows-msvc
```

Готовый файл появится в `app/target/x86_64-pc-windows-msvc/release/TwoWindowsInOne.exe`. GitHub Actions публикует архив после отправки тега.

## Сайт

`site/` содержит статическую часть сайта. Адрес API для GitHub Pages задаётся в `site/settings.js`. GitHub Pages подключается через **Settings → Pages → Source → GitHub Actions**.

`backend/` содержит сервер CAPTCHA и подписанных ключей. `npm ci && npm run build` собирает Worker в `dist/server/index.js`. Для самостоятельного размещения задайте `TWI_SITE_ORIGIN`, `TWI_CAPTCHA_SECRET` и `TWI_SIGNING_PRIVATE_PEM`. CAPTCHA использует 32 случайных байта в Base64. У закрытого ключа RSA 2048 в формате PKCS#8 должна быть открытая часть из `app/license-public.pem`. Закрытые ключи хранятся в секретах среды.
