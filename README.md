# Two Windows in One

Утилита запускает второй экземпляр программы в Windows Sandbox. Первое окно продолжает работать в основной системе, а у второго свои данные, процессы и профиль. Так можно, например, открыть другой аккаунт или попробовать настройки в чистой среде.

[Скачать Windows x64](https://github.com/qqew666/wo-Windows-in-One/releases/latest) · [Получить ключ](https://two-windows-in-one-key-api.sillypetunia8.chatgpt.site/)

## Запуск

1. Включите **Песочницу Windows** в компонентах Windows и перезагрузите ПК. Поддерживаются Windows Pro, Enterprise и Education с аппаратной виртуализацией. [Инструкция Microsoft](https://learn.microsoft.com/windows/security/application-security/application-isolation/windows-sandbox/windows-sandbox-install).
2. Распакуйте загруженный архив и запустите `TwoWindowsInOne.exe`.
3. Пройдите CAPTCHA на сайте и скопируйте ключ в окно приложения. В полночь по Москве истекает старый ключ и начинает действовать новый.
4. Выберите программу из списка открытых окон или укажите её файл `.exe`. Папка программы доступна песочнице только для чтения. Файл запускается из копии внутри песочницы.
5. Для программы, требующей установки, выберите режим **Установщик** и файл `.exe` или `.msi`. После установки откройте программу внутри песочницы. Для сетевого установщика, входа в аккаунт или отправки результатов в облако заранее включите **Интернет в песочнице**.

Windows Home не поддерживает Windows Sandbox. Одновременно можно открыть одну песочницу. При её закрытии файлы и настройки гостевой системы удаляются. Программы из Microsoft Store, приложения с драйверами и приложения, запрещающие работу в виртуальных машинах, могут быть несовместимы.

## Разработка и публикация

- `app/` — приложение Windows Forms для .NET 10. Опубликованная сборка включает среду выполнения.
- `site/` — статический сайт для GitHub Pages. Укажите адрес серверной части в `site/settings.js`, затем выберите **Settings → Pages → Source → GitHub Actions**.
- `backend/` — сервис CAPTCHA и подписанных ключей для среды Cloudflare Workers. `npm ci && npm run build` собирает `dist/server/index.js` с сервером и статическим сайтом. Для самостоятельного размещения укажите `TWI_SITE_ORIGIN` и задайте `TWI_CAPTCHA_SECRET` и `TWI_SIGNING_PRIVATE_PEM` в секретах среды.

`TWI_CAPTCHA_SECRET` — 32 случайных байта в Base64. `TWI_SIGNING_PRIVATE_PEM` — закрытый ключ RSA 2048 в формате PKCS#8. Его открытая часть должна совпадать с `app/license-public.pem`. Храните закрытые ключи за пределами репозитория.

Для выпуска новой версии отправьте тег вида `v1.0.1`. GitHub Actions собирает и публикует ZIP в Releases. Переменная репозитория `TWI_SITE_URL` задаёт адрес кнопки ключа для новых сборок.

```sh
dotnet publish app/TwoWindowsInOne.csproj -c Release -r win-x64 --self-contained true \
  -p:PublishSingleFile=true -p:EnableCompressionInSingleFile=true \
  -p:IncludeNativeLibrariesForSelfExtract=true -p:DebugType=None -o dist/windows
```
