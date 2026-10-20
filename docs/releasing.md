# Релизы и автообновление

Релизный workflow (`.github/workflows/release.yml`) собирает установщики по тегу `vX.Y.Z` и кладёт их
в **черновик** GitHub Release вместе с `latest.json` для автообновления (ADR-0012, ADR-0013).
Пока подпись обновлений не настроена, workflow намеренно падает на шаге «Check updater configuration».

## Однократная настройка

1. Сгенерируйте ключ подписи обновлений (minisign) — **на своей машине**, не в CI:

   ```bash
   pnpm --filter @dndstudio/desktop tauri signer generate -w ~/.tauri/dndstudio.key
   ```

   Приватный ключ и пароль никому не передавайте и не коммитьте. Потеря ключа = невозможность выпустить
   обновление для уже установленных копий.

2. В репозитории GitHub → Settings → Secrets and variables → Actions добавьте секреты:
   - `TAURI_SIGNING_PRIVATE_KEY` — содержимое файла приватного ключа;
   - `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` — пароль к нему.

3. В `apps/desktop/src-tauri/tauri.release.conf.json`:
   - `plugins.updater.pubkey` — содержимое `~/.tauri/dndstudio.key.pub`;
   - в `endpoints` замените `OWNER/REPO` на путь репозитория.

4. В корневом `Cargo.toml` поправьте `repository`.

## Выпуск версии

1. Поднимите версию в `Cargo.toml` (`[workspace.package] version`) и `apps/desktop/package.json`.
2. `git tag vX.Y.Z && git push origin vX.Y.Z`.
3. Дождитесь workflow, проверьте черновик релиза и установщики, опубликуйте релиз.

Клиент проверяет обновления через минуту после запуска и затем раз в 24 часа по адресу
`releases/latest/download/latest.json` — то есть видит только последний **опубликованный** релиз,
не черновики и не pre-release (stable-канал). Установка — только по подтверждению пользователя,
перед ней кампания сохраняется.

## Windows: WebView2

Установщик NSIS/MSI сам ставит WebView2 Runtime, если его нет (по умолчанию `downloadBootstrapper`).
Для разработки на Windows LTSC/IoT его нужно поставить вручную.
