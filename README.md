# Two Windows in One

Утилита для Windows x64. Она запускает второй экземпляр программы в Windows Sandbox. Первое окно остаётся в основной Windows, а второе получает отдельные процессы, реестр и папки пользователя.

## Запуск

1. Включите **Песочницу Windows** в компонентах Windows и перезагрузите ПК. Нужна поддерживаемая редакция Windows Pro, Enterprise или Education и включённая аппаратная виртуализация. [Инструкция Microsoft](https://learn.microsoft.com/windows/security/application-security/application-isolation/windows-sandbox/windows-sandbox-install).
2. Скачайте архив Windows x64 в разделе **Releases**, распакуйте и запустите `TwoWindowsInOne.exe`.
3. На сайте GitHub Pages нажмите **Показать ключ**, скопируйте его и активируйте в приложении.
4. Выберите приложение из списка открытых окон или выберите его `.exe`. Утилита передаст выбранную папку в Sandbox только для чтения и запустит копию из временной папки внутри Sandbox. Путь к исходной папке показывается перед запуском.
5. Для программ, которым нужна установка, выберите режим **Установщик** и файл `.exe` или `.msi`. После установки откройте программу внутри Sandbox. Для сетевого установщика или входа в аккаунт включите **Интернет в песочнице** перед запуском.

Ключ начинает действовать в 00:00 по Москве и истекает ровно через 24 часа. Активация проверяет цифровую подпись и время по часам компьютера. Ключ открывает запуск через эту утилиту. Операции Windows Sandbox, запущенные другими способами, обслуживаются самой Windows.

Windows Sandbox поддерживает одно гостевое окно одновременно. Настройки и файлы гостевой системы удаляются при её закрытии. Windows Home не поддерживается. Программы из Microsoft Store, программы с системными драйверами или с ограничениями на виртуальные машины могут быть несовместимы.

## Публикация на GitHub

В корне репозитория выполните команды для своего проекта. Требуются Node.js, OpenSSL и GitHub CLI с правом управления секретами репозитория.

```sh
umask 077
openssl genpkey -algorithm RSA -pkeyopt rsa_keygen_bits:2048 -out twi.private.pem
openssl pkey -in twi.private.pem -pubout -out app/license-public.pem
gh secret set TWI_SIGNING_PRIVATE_PEM < twi.private.pem
```

Открытый ключ `app/license-public.pem` попадёт в коммит и в приложение. Закрытый ключ `twi.private.pem` храните отдельно. При его замене выпустите приложение с новым открытым ключом, иначе старые сборки не распознают новые ключи.

В настройках репозитория выберите **Settings → Pages → Source → GitHub Actions**. Запушьте `main` и при необходимости запустите **GitHub Pages and daily keys** вручную в Actions. Сайт публикует подписанные ключи на текущие и следующие московские сутки, чтобы смена ключа в полночь не зависела от точного времени запуска Actions. Публичный репозиторий нужен для GitHub Pages на бесплатном тарифе. GitHub может приостанавливать расписание публичного репозитория после 60 дней без активности.

Чтобы появился файл для скачивания, отправьте тег версии, например `v1.0.0`. Workflow **Windows build** соберёт и опубликует `Two-Windows-in-One-win-x64.zip` в Releases. При использовании собственного домена установите переменную репозитория `TWI_SITE_URL` до сборки релиза.

## Сборка и локальный сайт

Нужен SDK .NET 10. Для локальной сборки запишите свой URL выдачи ключей в `app/site-url.txt`.

```sh
dotnet publish app/TwoWindowsInOne.csproj -c Release -r win-x64 --self-contained true \
  -p:PublishSingleFile=true -p:EnableCompressionInSingleFile=true \
  -p:IncludeNativeLibrariesForSelfExtract=true -p:DebugType=None -o dist/windows
TWI_SIGNING_PRIVATE_PEM="$(cat twi.private.pem)" node scripts/make-keys.mjs
python3 -m http.server 8047 --directory site
```

Откройте `http://localhost:8047`, чтобы проверить выдачу ключа. Для запуска гостевой программы нужен компьютер с доступной Windows Sandbox.
