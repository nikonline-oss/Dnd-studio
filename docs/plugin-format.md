# Формат плагина `.dndplugin`

## Цветовые темы (P1, ADR-0030)

Пакет `kind: "theme"` содержит `theme.json`:

```json
{ "light": { "--color-accent": "#6750a4" }, "dark": { "--color-accent": "#c8b6ff" } }
```

Разрешены только 16 переменных `THEME_COLOR_VARIABLES` из `dnd-protocol/src/theme.rs`,
соответствующих `--color-*` в штатном `tokens.css`. Значения — HEX `#rgb`, `#rrggbb`
или `#rrggbbaa`. Хотя бы одна палитра должна содержать цвет. Остальные цвета наследуются
от штатного оформления. Неизвестные поля, CSS-код, URL, шрифты и размеры запрещены.

Установка хранит тему в кампании; выбор через меню «Палитра» действует только на текущем
устройстве и доступен всем ролям. Пример — `plugins/theme-midnight`, встроенный в приложение.
Старые DTO без палитры читаются, но не предлагаются для выбора оформления.

Справочник формата пакета плагина (CLAUDE.md §8, ADR-0005). Источник истины — крейт
`crates/dnd-plugin`: мета-схемы в `crates/dnd-plugin/schemas/`, проверки в `src/`, примеры
пакетов — в его тестах (`tests/common/mod.rs`). Версия формата — **1**.

Плагины декларативны и **не исполняют код** (CLAUDE.md §9): в пакете только JSON и картинки.

## Пакет

`.dndplugin` — ZIP (сжатие deflate или без сжатия). Допустимы **только** такие имена:

| Имя                            | Что                                                    | Обязателен                      |
| ------------------------------ | ------------------------------------------------------ | ------------------------------- |
| `manifest.json`                | манифест                                               | да                              |
| `system.json`                  | правила системы                                        | только и всегда у `system`      |
| `sheet/<имя>.json`             | схема данных и раскладка листа персонажа               | те, что названы в `system.json` |
| `compendia/<имя>.json`         | компендий                                              | нет                             |
| `triggers/<имя>.json`          | шаблон триггера (ADR-0024)                             | нет                             |
| `assets/<sha256>.webp`, `.png` | картинка; имя — SHA-256 содержимого (64 hex, строчные) | нет                             |

`<имя>` — `[a-z0-9][a-z0-9._-]*\.json`, не длиннее 64 символов, без подпапок. Папки-записи
`sheet/`, `compendia/`, `triggers/`, `assets/` допускаются. Любое другое имя (в том числе `../x`, абсолютные
пути, `README.md`, `.css`, `.js`, `.svg`) — отказ `unexpected_file`: имена сверяются целиком по
белому списку, поэтому выйти за пределы пакета нельзя по построению. Файлы в `sheet/`, на которые
не ссылается `system.json`, тоже отказ.

Идентичность пакета — SHA-256 всего файла `.dndplugin`. Плагины хранятся **внутри кампании**:
пакет копируется в неё по хэшу, правила системного плагина становятся игровым состоянием,
поэтому сервер сессий получает их вместе с кампанией.

### Безопасное чтение (CLAUDE.md §9)

- Лимиты (`PackageLimits::DEFAULT`, настраиваются вызывающим): пакет ≤ 256 МБ; ≤ 5000 записей;
  JSON-документ ≤ 16 МБ; картинка ≤ 32 МБ; всё распакованное ≤ 512 МБ. Размеры считаются **по
  реально прочитанным байтам**, а не по заголовкам ZIP (zip-бомбы).
- Неоднозначный ZIP — отказ `invalid_archive`: повторяющиеся имена, локальные заголовки,
  расходящиеся с центральным каталогом, перекрывающиеся записи.
- Зашифрованные записи и символические ссылки — `unsafe_entry`.
- Картинка должна совпадать с хэшем в имени (`hash_mismatch`) и быть тем форматом, что указан в
  расширении (`invalid_asset`, проверяется сигнатура). SVG не допускается вовсе.
- Каждый JSON-документ проверяется встроенной мета-схемой (draft 2020-12, неизвестные ключи
  запрещены), затем — проверками, которые схемой не выразить. `$ref` наружу не разрешаются:
  валидатор собран без загрузки по сети и с диска.
- Подписи — этап 5. Пока каждый локальный пакет — «неподписанный» (UI предупреждает).

## Тексты (`Label`)

Везде, где текст видит пользователь (имя плагина, подписи, `title` в схеме листа, `text` в
раскладке), допустима строка или объект по языкам:

```json
"Сила"
{ "ru": "Сила", "en": "Strength" }
```

В объекте — хотя бы один из `ru`, `en`, другие ключи запрещены. Показ: нужный язык → `en` → `ru`.
Строка — до 200 символов (узел `text` — до 4000).

## `manifest.json`

```json
{
  "format": 1,
  "id": "system-srd5",
  "name": { "ru": "SRD 5.2", "en": "SRD 5.2" },
  "version": "1.0.0",
  "kind": "system",
  "authors": ["DndStudio contributors"],
  "license": "CC-BY-4.0",
  "attribution": "This work includes material from the System Reference Document 5.2 by Wizards of the Coast LLC, licensed under CC-BY-4.0.",
  "app_compat": "^0.1",
  "dependencies": {}
}
```

| Поле           | Тип                                   | Правила                                                          |
| -------------- | ------------------------------------- | ---------------------------------------------------------------- |
| `format`       | число                                 | `1`; другое число → `unsupported_format` («обновите приложение») |
| `id`           | строка                                | `[a-z0-9][a-z0-9.-]*`, ≤ 64                                      |
| `name`         | Label                                 |                                                                  |
| `version`      | строка                                | semver: `1.0.0`, `2.1.0-rc.1`                                    |
| `kind`         | `system` `content` `theme` `ui`       | системный — ровно один на кампанию                               |
| `authors`      | массив строк                          | 1–32                                                             |
| `license`      | строка                                | SPDX-выражение (не сверяется со списком SPDX)                    |
| `attribution`  | строка, необязательно                 | текст атрибуции (CC-BY и т. п.), ≤ 4000                          |
| `app_compat`   | строка                                | диапазон версий приложения                                       |
| `dependencies` | `{ id: диапазон }`, по умолчанию `{}` | ≤ 32; зависеть от себя нельзя                                    |

### Версии и диапазоны

Используется semver в варианте Cargo (крейт `semver` — де-факто стандарт, на нём работает Cargo):

| Диапазон      | Значит                                    |
| ------------- | ----------------------------------------- |
| `^1.2`, `1.2` | `>=1.2.0, <2.0.0`                         |
| `^0.1`        | `>=0.1.0, <0.2.0` (для 0.x — по минорной) |
| `~1.2`        | `>=1.2.0, <1.3.0`                         |
| `>=0.1, <2`   | явные границы через запятую               |
| `=1.0.0`, `*` | точно эта / любая                         |

- `app_compat` сверяется с версией приложения **без** pre-release части: бета `0.1.0-beta.2`
  загружает плагины, написанные для `0.1.0`.
- Зависимость в pre-release версии (`2.0.0-rc.1`) подходит только диапазону, который явно
  упоминает pre-release (`^2.0.0-rc`), — как в Cargo.

### Совместимость и зависимости

- Установка (`check_install`): плагин подходит приложению, его зависимости установлены в
  подходящих версиях, второй системный плагин не ставится (`multiple_systems`; плагин с тем же
  `id` — это обновление).
- Активация (`resolve`): несовместимые деактивируются с причиной — `incompatible_app`,
  `missing_dependency`, `dependency_version_mismatch`; затем каскадом все, чья зависимость
  деактивирована (`dependency_inactive`). Циклы зависимостей допустимы. Повтор `id` —
  `duplicate_plugin`.

## `system.json` (только `kind: "system"`)

Необязательное поле `creature_sheet_layout` задаёт путь к отдельной декларативной форме NPC,
например `"creature_sheet_layout": "sheet/creature.layout.json"`. Файл должен отличаться от
основного layout и схемы. Загрузчик проверяет те же типы компонентов, пути полей, глубину и
ссылки на computed/rolls/resources, что для основного листа. Данные по-прежнему соответствуют
единственной `sheet.schema`; формулы, ресурсы и проверки ядра остаются общими для системы.

Редактор выбирает эту форму для `kind: "npc"`, включая созданных из компендия существ;
PC использует основной `sheet.layout`. При отсутствии дополнительной формы NPC тоже использует
основную. Выбор сохраняется через существующий kind персонажа, без новых полей персонажа или
миграции БД. Это одна форма существ на систему, а не отдельная схема для каждого компендия.
Обновление системного плагина обновляет и представление существующих NPC.

```json
{
  "sheet": { "schema": "sheet/character.schema.json", "layout": "sheet/character.layout.json" },
  "computed": {
    "str_mod": "floor((@str - 10) / 2)",
    "prof": "if @level >= 17 then 6 else if @level >= 13 then 5 else if @level >= 9 then 4 else if @level >= 5 then 3 else 2"
  },
  "initiative": "1d20 + @dex_mod",
  "resources": [
    {
      "id": "hp",
      "label": { "ru": "Хиты", "en": "Hit points" },
      "value": "@hp_current",
      "max": "@hp_max"
    }
  ],
  "health_thresholds": [75, 25, 0],
  "rolls": [
    {
      "id": "str_check",
      "label": { "ru": "Проверка Силы", "en": "Strength check" },
      "dice": "1d20 + @str_mod"
    }
  ]
}
```

| Поле                | Правила                                                                                                                           |
| ------------------- | --------------------------------------------------------------------------------------------------------------------------------- |
| `sheet`             | `schema` и `layout` — разные файлы `sheet/<имя>.json` в пакете                                                                    |
| `computed`          | имя (`[A-Za-z_][A-Za-z0-9_]*`, ≤ 64) → выражение (≤ 1024); ≤ 256; имя не совпадает с полем верхнего уровня схемы и с `id` ресурса |
| `initiative`        | выражение костей (≤ 256)                                                                                                          |
| `resources`         | ≤ 32; `id` — идентификатор, уникален; `value`, `max` — выражения                                                                  |
| `health_thresholds` | 1–8 целых 0–100, строго по убыванию; по умолчанию `[75, 25, 0]` — границы Healthy / Wounded / Critical / Dead                     |
| `rolls`             | ≤ 256; `id` уникален; `dice` — выражение костей (≤ 256)                                                                           |
| `conditions`        | ≤ 64 состояний `{ id, label }`; `id` — `[a-z0-9_-]{1,64}`, уникален (ADR-0024)                                                    |
| `inventory`         | `{ path, name_field }`: `path` ведёт к массиву объектов в схеме листа, `name_field` — их строковое поле с названием предмета      |

`computed`, `resources`, `rolls`, `conditions` необязательны (пусто), `inventory` — тоже (без него
нет выдачи предметов и события «получен предмет»). Бросок кнопкой листа передаёт ядру `id` броска: ядро
бросает формулу системы, а не присланную клиентом. **Синтаксис выражений и костей здесь не
проверяется**: пакет хранит их строками, ядро разбирает их `dnd-expr`/`dnd-dice` при применении
правил и отклоняет некорректные (волна 2). Проверяются только лимиты длины.

### Язык выражений (`dnd-expr`, кратко)

- Числа — **рациональные** (точная арифметика: `7 / 2` — это 7/2, без ошибок округления float).
- Операторы: `+ - * /`, унарный `-`, сравнения `== != < <= > >=`, скобки.
- Условие: `if <условие> then <a> else <b>` (вкладывается).
- Функции (только эти): `min(a, b, …)`, `max(a, b, …)`, `floor(x)`, `ceil(x)`, `round(x)`, `abs(x)`.
- Ссылки: `@имя`, `@имя.поле` — поля листа, вычисляемые значения, ресурсы. Доступно только то,
  что ядро передало в контекст; циклов, пользовательских функций и I/O нет.
- Лимиты: длина ≤ 1024 символа, глубина вложенности ≤ 32.
- Результат вычисляемого поля (`computed`) обязан быть **целым** — для дробей используйте
  `floor`/`ceil`/`round`.

Кости (`initiative`, `rolls`) — нотация `dnd-dice` (`1d20 + @str_mod`, `4d6kh3`, `d%`,
`1d20 adv`), см. `crates/dnd-dice/README.md`.

## Схема данных листа (`sheet/*.schema.json`)

**Подмножество JSON Schema**, которое ядро проверяет само. Корень — `"type": "object"`.
Разрешены только эти ключевые слова, и только для подходящего типа:

| Тип       | Ключевые слова (кроме `type`, `title`, `default`) |
| --------- | ------------------------------------------------- |
| `object`  | `properties`, `required`                          |
| `integer` | `minimum`, `maximum`, `enum`                      |
| `string`  | `maxLength`, `enum`                               |
| `boolean` | —                                                 |
| `array`   | `items` (обязательно), `maxItems`                 |

Всё остальное (`$ref`, `pattern`, `oneOf`, `number`, `null`, массив типов, `additionalProperties`…)
— отказ `unsupported_schema`. Дополнительно:

- имена свойств — идентификаторы `[A-Za-z_][A-Za-z0-9_]*` (≤ 64): на них ссылаются `@имя`;
- `required` — только объявленные свойства; `minimum ≤ maximum`;
- целые — в пределах ±(2^53−1) (ADR-0016); `maxLength` ≤ 65536; `maxItems` ≤ 1000; `enum` — 1–256
  значений, каждое подходит своему узлу; `default` подходит своему узлу;
- глубина вложенности ≤ 16; ≤ 256 свойств на объект;
- `title` — Label (а не только строка, как в JSON Schema).

Семантика проверки данных (`dnd_plugin::check_data` — эталон, ядро повторяет её у себя): у объекта
нет свойств сверх объявленных; `required` обязательны; длина строки — в символах Unicode.

```json
{
  "type": "object",
  "required": ["str"],
  "properties": {
    "name": { "type": "string", "maxLength": 100, "title": { "ru": "Имя", "en": "Name" } },
    "str": { "type": "integer", "minimum": 1, "maximum": 30, "default": 10 },
    "hp_current": { "type": "integer", "minimum": 0, "default": 0 },
    "hp_max": { "type": "integer", "minimum": 0, "default": 0 },
    "inspired": { "type": "boolean", "default": false },
    "alignment": { "type": "string", "enum": ["good", "neutral", "evil"] },
    "items": {
      "type": "array",
      "maxItems": 50,
      "items": {
        "type": "object",
        "properties": {
          "title": { "type": "string" },
          "count": { "type": "integer", "minimum": 0 }
        }
      }
    }
  }
}
```

## Раскладка листа (`sheet/*.layout.json`)

Дерево узлов `{ "type": ..., ... }`, корень — один узел. Глубина ≤ 32, у контейнера ≤ 256 детей.
Неизвестные типы и ключи — отказ `invalid_layout`.

| `type`     | Поля                                 | Смысл                                 |
| ---------- | ------------------------------------ | ------------------------------------- |
| `tabs`     | `tabs: [{ label, children }]` (1–32) | вкладки                               |
| `section`  | `label?`, `children`                 | блок с заголовком                     |
| `row`      | `children`                           | в строку                              |
| `grid`     | `columns` (1–12), `children`         | сетка                                 |
| `field`    | `path`, `label`, `widget?`           | редактируемое поле                    |
| `computed` | `name`, `label`, `format?`           | вычисляемое значение из `system.json` |
| `roll`     | `roll`, `label?`                     | кнопка броска из `rolls`              |
| `resource` | `resource`                           | ресурс (значение / максимум)          |
| `list`     | `path`, `label`, `item: [узлы]`      | список объектов                       |
| `text`     | `text` (Label, ≤ 4000)               | статичный текст                       |

Проверки ссылок:

- `field.path` — путь через точку (`abilities.str`) к полю схемы типа `integer`, `string` или
  `boolean`. Внутри `list.item` пути — **относительно элемента** списка.
- `widget`: `integer` → `number` (по умолчанию) или `select`; `string` → `text` (по умолчанию),
  `textarea` или `select`; `boolean` → `checkbox` (по умолчанию). `select` — только у полей с `enum`.
- `list.path` — массив, `items` которого — объект.
- `computed.name`, `roll.roll`, `resource.resource` — существуют в `system.json`.
- `computed.format`: `plain` (по умолчанию) или `signed` — со знаком (`+3`, `−1`), для бонусов и
  модификаторов.

```json
{
  "type": "tabs",
  "tabs": [
    {
      "label": { "ru": "Основное", "en": "Main" },
      "children": [
        {
          "type": "row",
          "children": [
            { "type": "field", "path": "str", "label": "STR", "widget": "number" },
            { "type": "computed", "name": "str_mod", "label": { "ru": "мод.", "en": "mod" } },
            { "type": "roll", "roll": "str_check" }
          ]
        },
        { "type": "field", "path": "alignment", "label": "Alignment", "widget": "select" },
        { "type": "resource", "resource": "hp" }
      ]
    },
    {
      "label": { "ru": "Снаряжение", "en": "Gear" },
      "children": [
        {
          "type": "list",
          "path": "items",
          "label": "Items",
          "item": [
            { "type": "field", "path": "title", "label": "Item" },
            { "type": "field", "path": "count", "label": "#" }
          ]
        }
      ]
    }
  ]
}
```

## Компендии (`compendia/*.json`)

Любой вид плагина. Здесь только разбираются и проверяются, показываются — на этапе 3b.

```json
{
  "id": "spells",
  "name": { "ru": "Заклинания", "en": "Spells" },
  "entry_type": "spell",
  "entries": [
    { "id": "fireball", "name": { "ru": "Огненный шар", "en": "Fireball" }, "data": { "level": 3 } }
  ]
}
```

`id` компендия и записей — `[a-z0-9][a-z0-9._-]*` (≤ 64), уникальны (компендии — в пакете,
записи — в компендии); `entry_type` — идентификатор; `data` — объект; ≤ 10 000 записей.

### Персонаж из записи компендия

Необязательное зарезервированное поле `data.character_template` позволяет GM/Co-GM создать
независимого персонажа и затем разместить связанный токен на выбранной карте:

```json
{
  "character_template": {
    "name": { "ru": "Разведчик", "en": "Scout" },
    "kind": "npc",
    "sheet_data": { "attributes": { "body": 1, "agility": 2, "mind": 1, "spirit": 0 } }
  }
}
```

`kind` (`pc` или `npc`) и объект `sheet_data` обязательны; `name` — необязательная метка
в формате `Label` (по умолчанию имя записи). Другие ключи шаблона запрещены. Остальные поля
`data` по-прежнему произвольные. Числа листа — безопасные целые; действуют лимиты `sheet_json`.
В системном пакете загрузчик также проверяет лист по его схеме после применения defaults.
В content-пакете схема целевой системы недоступна загрузчику: укажите зависимость от системы
в манифесте; при создании ядро проверит данные по схеме активной системы кампании.

Используются существующие `CreateCharacter` и `CreateToken`. NPC не имеет владельца; PC
принадлежит создающему GM/Co-GM. Лист и его layout берутся из активного системного плагина:
шаблон задаёт начальные данные, а не отдельную схему. Обновление записи не меняет созданные
экземпляры. Создание персонажа и токена — два независимых подтверждённых действия: при отказе
размещения персонаж сохраняется, можно повторить размещение или открыть его лист. Токен
появляется в центре карты, связан с персонажем и далее перемещается обычным способом.
Пример без SRD-механик: `plugins/system-generic-d6/compendia/characters.json`.

Без `character_template` действие также доступно: создаётся NPC с именем записи и пустыми
данными `{}`, которые ядро заполняет defaults активной системы. UI поясняет, что лист нужно
отредактировать; поля исходной записи автоматически не интерпретируются и не меняются.
Если шаблон указан, но некорректен, создание блокируется с ошибкой, без перехода к пустому листу.

В панели компендия можно изменить имя создаваемого экземпляра и посмотреть его форму перед
созданием. Предпросмотр использует общий renderer, не допускает изменений и бросков; поля
`character_template` не выводятся как технический JSON в тексте записи. Данные оригинала
не меняются. Примеры обеих форм есть в Generic d6 и SRD, а SRD-компендий монстров содержит
явные `character_template` с начальными данными листов.

## Шаблоны триггеров (`triggers/*.json`)

Готовый триггер «Когда → Если → Тогда» (ADR-0024) с пропусками. Это данные, а не код: плагин ничего
не исполняет. Не больше 64 шаблонов, `id` уникальны.

```json
{
  "id": "hidden_find",
  "name": { "ru": "Скрытая находка", "en": "Hidden find" },
  "description": { "ru": "…", "en": "…" },
  "params": [
    { "id": "zone", "label": { "ru": "Зона", "en": "Zone" }, "kind": "zone" },
    { "id": "dc", "label": { "ru": "Сл", "en": "DC" }, "kind": "number" }
  ],
  "definition": {
    "name": "Hidden find",
    "enabled": true,
    "once": true,
    "event": { "type": "roll_in_zone", "zone": "{{zone}}", "rolls": ["perception"] },
    "tokens": [],
    "condition": "@total >= {{dc}}",
    "steps": [{ "when": null, "action": { "type": "chat", "text": "…", "gm_only": false } }]
  }
}
```

- `params[].kind`: `zone`, `board_node`, `board`, `map`, `journal`, `token` (мастер выбирает элемент
  кампании), `roll`, `resource`, `condition` (из правил системы), `number`, `text`.
- `definition` — определение триггера (как в протоколе: `event`, `steps` с действиями `reveal`,
  `set_node_visibility`, `chat`, `roll`, `change_resource`, `add_condition`, `remove_condition`,
  `give_item`, `play_sound`, `stop_sound`, `play_sting`, `spawn_tokens`, `start_combat`,
  `apply_effect`), в строках которого `{{id}}` заменяется значением параметра. Пропуск, составляющий
  строку целиком (`"turns": "{{turns}}"`), у параметра `kind: number` подставляется числом —
  так задаются длительности и количества. Пакет проверяет только форму шаблона; заполненный
  триггер ядро проверяет как любой другой.
- `apply_effect` вешает на токен временный эффект: `label` (подпись), опционально `condition`
  (id состояния системы) и/или периодическое изменение ресурса — `resource` + `dice` + `heal`
  (в начале каждого хода токена), `duration`: `{ "type": "rounds" | "turns", "rounds" | "turns": N }`
  или `{ "type": "real_time", "minutes": N }`.

## Ассеты (`assets/`)

`assets/<sha256>.webp` или `.png`, имя — SHA-256 содержимого. Конфликтов не бывает, одинаковые
картинки разных плагинов хранятся один раз.

## Коды ошибок

`PluginError::code()` — стабильные строки для UI:

| Код                           | Когда                                                 |
| ----------------------------- | ----------------------------------------------------- |
| `io`                          | не прочитать файл                                     |
| `invalid_archive`             | не ZIP, повреждён, неоднозначен                       |
| `too_large`                   | превышен лимит размера или числа файлов               |
| `unexpected_file`             | имя вне белого списка / лишний файл                   |
| `unsafe_entry`                | зашифрованная запись или симлинк                      |
| `missing_file`                | нет `manifest.json`, `system.json` или файла листа    |
| `hash_mismatch`               | картинка не совпадает с хэшем в имени                 |
| `invalid_asset`               | содержимое — не тот формат картинки                   |
| `unsupported_format`          | `format` манифеста новее поддерживаемого              |
| `invalid_manifest`            | манифест не соответствует формату                     |
| `invalid_document`            | `system.json` или компендий не соответствует формату  |
| `unsupported_schema`          | схема листа вне подмножества                          |
| `invalid_layout`              | раскладка некорректна или ссылается на несуществующее |
| `incompatible_app`            | `app_compat` не подходит версии приложения            |
| `missing_dependency`          | зависимость не установлена                            |
| `dependency_version_mismatch` | зависимость установлена в неподходящей версии         |
| `dependency_inactive`         | зависимость деактивирована                            |
| `duplicate_plugin`            | один `id` дважды                                      |
| `multiple_systems`            | второй системный плагин в кампании                    |
| `internal`                    | ошибка в самом приложении                             |

## Что получает ядро

`PluginPackage::to_dto()` → `InstalledPluginDto { id, version, kind, name, license, attribution?,
archive_sha256, system? }`, где `system` — `SystemRulesDto { sheet_schema, sheet_layout, computed,
initiative, resources, health_thresholds, rolls }` (схема и раскладка — JSON как есть, без путей
пакета).

## Формат 2: настраиваемые правила и действия (ADR-0034)

Загрузчик читает форматы 1 и 2. Новые `features` и `mechanics` требуют
`manifest.format: 2`. Старые пакеты продолжают работать с пустой механикой.
Пример полного описания: `plugins/system-srd5/system.json`.

`features[]` содержит `id`, `label`, `description`, `default_enabled` (не более 128).
Кампания отдельно хранит `feature_settings`; пакет не задаёт пользовательские
переопределения. Настройки открываются нажатием имени установленного плагина.

`mechanics` содержит `actions`, `movement_rules`, `turn_resources` и
`movement_condition`. Пустые условия означают отсутствие ограничения.
Все выражения исполняются штатными `dnd-expr`/`dnd-dice`, скриптов нет.

Действие объявляет идентификатор, локализованное имя, флаг функции, условие,
дальность (выражение в клетках), метрику, необязательный радиус круглой области,
атаку или спасбросок, урон и ресурс цели, необязательную стоимость (один ресурс или список расходов)
исполнителя. `reaction_only` исключает действие из обычного меню.
`save_dc` вычисляется по исполнителю, `@difficulty` и `@total` передаются условию
успеха спасброска цели; успех делит урон на `save_divisor` с округлением вниз.
Для атак `@natural` — первый оставленный куб; `critical_if` и `critical_dice`
позволяют плагину описать дополнительные кости урона. `visual` объявляет HEX-цвет
и длительность 50–5000 мс. Карта показывает предварительную область и требует
подтверждения перед применением.

Правило перемещения объявляет `reach_cells`, `metric`, `source_condition`,
`target_condition`, `movement_modes`, `hostile_only`, `combat_only`, `feature`
и ссылку на действие. Ядро предлагает действие перед выходом из досягаемости.
Именно плагин решает, участвуют ли voluntary/forced/teleport перемещения.
`turn_resources` перечисляет ресурсы, которые восстанавливаются до максимума
в начале собственного хода. `movement_condition` ограничивает добровольное
движение и его возобновление после реакции.

Шаг триггера `{ "type": "plugin_action", "action": "fireball", "source": null,
"target": null, "x": 3200, "y": 3200 }` использует то же действие.
`source: null` означает токен-активатор; x/y — Units16 для центра области.
Для одиночного действия нужен `target`. Ограничения текущего среза — ADR-0034.

## Последствия состояний (ADR-0035)

В `mechanics.condition_rules` можно объявить правило:

```json
{
  "id": "prone_attacks",
  "label": { "ru": "Сбит с ног: помеха атакам", "en": "Prone: disadvantage on attacks" },
  "condition": "prone",
  "subject": "actor",
  "feature": "condition_rules",
  "when": "",
  "effects": [{ "type": "roll", "kind": "attack", "tags": [], "mode": "disadvantage" }]
}
```

`subject` выбирает носителя состояния. Условия видят `@actor.*`, `@target.*`,
`@actor.condition.ID`, `@target.condition.ID` и для атаки `@event.distance_cells`.
Ссылка на неизвестное поле или состояние отклоняется при установке.

Последствия:

- `deny`: `capability` action/bonus_action/reaction/movement/provoke;
- `roll`: `kind` attack/save/check/initiative, необязательные `tags` (все должны
  присутствовать у броска), `mode` advantage/disadvantage;
- `parameter`: числовой `path`, `add` (±1 000 000), необязательные `minimum`/`maximum`;
- `imply`: идентификатор дополнительного эффективного состояния `condition`.

Максимумы: 128 правил, 16 последствий на правило, 16 тегов. Указанные состояния
и флаги должны быть объявлены системой. Параметры должны быть числовыми полями
или вычисляемыми значениями; ресурсные значения и их зависимости исключены.
Числовые изменения суммируются, затем ограничиваются снизу и сверху; не сохраняются
в исходном листе. Формулы зависимых вычисляемых полей учитывают изменения.

У именованного броска `rolls[]` появились `kind` и `tags`; у действия — `tags`,
необязательная `activation` (action/bonus_action/reaction), `target_self` и
`turn_condition`. Реакция с прежним `reaction_only` по умолчанию имеет соответствующий
тип активации. У обычного действия тип по умолчанию action. `turn_condition`
применяется к себе до конца собственного текущего хода; проверяется до расходов.

Источник состояния может быть постоянным или временным. Наличие двух источников
не удваивает модификатор; удаление одного не отменяет другой. Режимы d20 собираются
со всех источников и с явной формулы; преимущество и помеха вместе дают обычный бросок.
Для модифицируемого теста формула должна содержать ровно один положительный одиночный d20.

### Multiple action costs

An action may declare `costs: [{"resource":"action","amount":"1"},
{"resource":"spell_slot_3","amount":"1"}]`. Amounts are integer expressions
against the actor's effective fields, evaluated before any expense or effect.
Each amount must be between 1 and 1,000,000. There are at most 16 entries;
repeated resource IDs are summed. All balances are checked before spending.
A failed action emits no events and spends nothing. `cost` remains supported
as a one-unit expense for older packages, but cannot be combined with nonempty
`costs`. The bundled Fireball consumes both an action and a third-level slot.

### GM dashboard

`mechanics.gm_panel` declares up to 24 ordered cards. Each card has unique `id`,
`label`, `description`, `kind` and optional `items`. Kinds:

| Kind         | Content                                          | `items`                 |
| ------------ | ------------------------------------------------ | ----------------------- |
| `overview`   | Scene counts and combat tracker                  | Empty                   |
| `reactions`  | Pending movement reactions across the campaign   | Empty                   |
| `characters` | Searchable scene roster and sheet links          | Empty                   |
| `rules`      | Campaign switches                                | Feature IDs             |
| `actions`    | Actor/target selection and explicit confirmation | Non-reaction action IDs |

Labels are limited to 200 bytes per language, descriptions to 2,000, references
to 128 per card. Only GM and co-GM see the dashboard. Cards use ordinary React
controls, never plugin HTML or scripts. Area shortcuts center on a chosen token;
free placement and geometric preview are available on the map. Existing kernel
permissions and resource checks apply to every dashboard command.

### Damage profiles

Actions optionally set `damage_type: "fire"` and
`damage_profile: "standard_damage"`. Types are opaque identifiers, not kernel enums.
`mechanics.damage_profiles` holds up to 32 profiles:

```json
{
  "id": "standard_damage",
  "label": "Damage",
  "feature": "damage_rules",
  "modifiers": [
    {
      "id": "resist_fire",
      "types": ["fire"],
      "condition": "@defenses.resistance.fire || @defenses.resistance.all",
      "amount": "floor(@damage / 2)"
    }
  ],
  "absorb": ["temporary_hp"]
}
```

A modifier's condition and amount use effective target fields and reserved
`@damage`. Empty `types` matches all types, an empty condition always applies.
Modifiers run in declaration order **after** saving-throw reduction and critical
extra damage. Each result must be a nonnegative JS-safe integer. At most 64
modifiers and 32 type selectors per modifier are accepted. Each modifier ID is
unique within its profile. The plugin is responsible for ordering and avoiding
stacking: the SRD sample combines `all` and a specific type in a single condition.

After transformations, up to eight unique resource IDs in `absorb` take damage
in order; any remainder goes to the action's `resource`. That final resource
cannot also appear in `absorb`. Existing resource limits apply, zero losses emit
no resource event. Costs, all targets and buffers commit atomically. Hidden
resource announcements remain GM-only. A disabled profile feature or an action
without a profile uses the legacy direct resource deduction. This is a damage
consumption pipeline, not an automatic temporary-resource grant or rest system.

### Maintained effects (protocol 14)

`mechanics.maintained_rules` contains up to 32 policies:

```json
{
  "id": "focus",
  "label": "Focus save",
  "feature": null,
  "exclusive_group": "focus",
  "keep_condition": "@hp.current > 0 && !@condition.incapacitated",
  "damage_check": {
    "resource": "hp",
    "dice": "1d20 + @con_save",
    "difficulty": "min(30, max(10, floor(@damage / 2)))",
    "success": "@total >= @difficulty",
    "tags": ["con", "concentration"]
  }
}
```

An action may set `maintain: {"rule":"focus","condition":"ward",
"duration":{"type":"rounds","rounds":100}}`. It must have `damage_dice: "0"`,
no attack/save, no reaction-only flag and no `turn_condition`. All valid selected
character tokens receive linked ordinary effects. Supported durations are the
existing `rounds`, `turns` and `real_time` variants with their normal bounds.
A new effect replaces the owner's previous effect in the same `exclusive_group`.
No more than 128 maintained records exist globally; normal per-token effect
limits apply. Labels that become effect names obey the effect label limit.

`keep_condition` reads effective owner fields and reserved `@condition.ID` flags.
The check's `difficulty` additionally reads reserved `@damage`; `success` reads
`@damage`, `@difficulty` and `@total`. Dice read ordinary owner fields and apply
Save modifiers filtered by at most 16 tags. Amounts must be JS-safe integers;
difficulty must be nonnegative. Runtime errors reject the complete action.
The kernel validates references and evaluates declarations on a default sheet.

Damage checks use the amount after save/reduction/resistance and before buffers
and HP clamping, only for the declared primary resource. Zero damage does not
roll. Direct costs, manual resource edits and old untyped trigger damage are not
classified as typed damage.

`PluginRuntime.maintained` stores ID, owner character, source token, rule, label
and child effect IDs. `end_maintained_effect` accepts the record ID, requires the
owner or GM/co-GM and removes linked effects atomically. Lifecycle cleanup handles
source deletion, child expiry, system rule changes and disabled policies. The GM
panel's new `maintained` card has empty `items`. Players only receive their own
records and visible child references. Existing SQLite runtime storage persists it.

The SRD example uses combat rounds for Shield of Faith; outside combat its game
clock is adjudicated manually. It does not implement a full spellcasting or
interrupt stack. Other plugins may choose an explicit real-time duration instead.

A plugin description is limited to `MAX_PLUGIN_BYTES` (512 KiB). In session play,
GM/co-GM `InstallPlugin` messages may use this bound plus 4 KiB for the envelope;
ordinary messages keep the configured WebSocket limit (64 KiB by default).
Player/spectator transports do not receive the expanded bound. See ADR-0040.

### Interruptible actions (protocol 15)

`ActionCost.on_resolution` defaults to false. True costs are checked and reserved
at declaration and paid only if the action resolves. Quantities are evaluated
once, before any expenses. Other pending actions of that character subtract all
reservations from available balances. Duplicate resource costs still aggregate
for affordability and payment. Costs without the flag retain existing semantics.

`mechanics.interruption_rules` accepts up to 128 declarations:

```json
{
  "id": "interrupt_cast",
  "feature": "counterspell",
  "action": "counterspell",
  "tags": ["spell"],
  "any_tags": ["verbal", "somatic", "material"],
  "range_cells": "12",
  "metric": "chebyshev",
  "source_condition": "@automation.can_counterspell && @automation.can_see",
  "target_condition": "@hp.current > 0",
  "hostile_only": false
}
```

All `tags` must match the declared action; at least one `any_tags` must match,
unless that list is empty. Selectors have at most 16 identifiers each. Conditions
read the reacting and declaring character respectively. Eligibility, reaction
capability and all costs are checked separately; conditions should describe
capabilities rather than restating costs that have already been paid. Range is
0–1000 cells and accounts for token scales. Player-owned reactors need both
source tokens visible. Visibility to a creature is otherwise adjudicated manually.

The referenced reaction-only action sets, for example:

```json
{
  "save_dice": "1d20 + @con_save",
  "save_dc": "@automation.spell_dc",
  "cancel_if": "@total < @difficulty",
  "tags": ["con", "counterspell"]
}
```

This is the action's `interrupt` field. It cannot combine with attack/save fields,
maintenance, turn conditions, or nonzero damage. The pending caster makes the
Save-kind roll with these tags against the reactor's DC. The expression then
reads target effective fields plus `@total` and `@difficulty`. On true the parent
frame is countered, otherwise it resumes its remaining offers. Counteractions
can themselves open windows. Manual execution of interrupt-only actions is
rejected. They also cannot be movement-hook actions.

Runtime stores `action_stacks` with map, unique current frame IDs, original player
controller, fixed deferred costs, selected point/target, current offers and
`ready`/`awaiting_reactions` flags. Limits: 16 stacks, depth 8, 128 offers per
frame, one stack per map. `resolve_action_reaction` requires stack/frame/source
and accept; owner or GM/co-GM may answer the current offer. `continue_plugin_action`
requires an actually empty top queue and its owner or GM. `cancel_action_stack`
is GM-only. Config changes, invalid source identity and disabled action features
invalidate waiting work; paid costs remain spent and reservations are released.

Ordinary moves and turns cannot bypass the window. Hidden frame details, foreign
reservations and other casters' selected points are removed from player snapshots;
an empty public map lock contains no caster identity. A filtered-out queue is
still marked `awaiting_reactions`, so a player cannot continue past others.
Outcomes use localizable `plugin_action_resolved` system messages.

The shared plugin action executor also works from triggers, but this does not
suspend the rest of a multi-step trigger workflow. Use a standalone plugin action
when a later step would depend on its resolved effects. Full spell-slot-per-turn
economy and line-of-sight geometry are outside this slice. See ADR-0041.

### Turn economy and trigger continuations (protocol 16)

`mechanics.turn_budgets` declares up to 32 policies with unique resource IDs:

```json
{
  "resource": "spell_cast",
  "label": "Slot expenditure",
  "feature": "turn_economy",
  "refresh": "every_turn",
  "amount": null
}
```

`own_turn` refreshes at the actor's own turn; `every_turn` refreshes for every
character on the scene when any turn starts. Combat start initializes both.
An optional integer `amount` expression sets the desired balance; null means
resource maximum. It must evaluate in 0..max. The resource must read a writable
integer field, not a computed expression. Invalid runtime refill evaluations
leave the balance unchanged. Refills run before periodic turn effects and read
the latest sheet after each write, so resources do not overwrite each other.
Legacy `turn_resources` still handles resources without an active new policy.

Actions may set `timing: {"turn":"own","feature":"turn_economy"}` or `other`.
This constrains combat timing only; absent/disabled timing remains unrestricted.
An ActionCost may set `scope: "combat"` and `feature: "turn_economy"`; scope defaults
to `always`, feature to null. A disabled or out-of-scope cost is neither checked,
reserved nor paid. Deferred costs preserve the evaluation at declaration.

The SRD sample declares own-turn budgets for action/bonus/reaction and an
every-turn budget for spell-slot expenditure. Its spell costs reserve that
budget and the actual slot until execution; cancelled spells release both.
Slot-free spell variants omit both costs. Configurable per-turn quantities,
amount expressions, different timing and alternate costs support system-specific
exceptions and extra actions; restricted class actions need their own declarations.

SheetView returns an optional `economy` derived view with combat/turn flags and
budget balances (available, reserved, refill target and scope). The core computes
it; clients display it without evaluating formulas. No hidden turn actor name is
included. The view is refreshed on combat, sheet, rule and reservation changes.

A trigger's plugin_action now pauses subsequent steps when it opens a reaction
window. Runtime `trigger_runs` records the root ID, definition version, activation
context, next step and variables. After completion/countering, `when` may branch
using `@action.completed == 1` or `@action.countered == 1`. Master cancellation,
invalid execution, edited/disabled/deleted definition or missing context aborts
the remainder. Previously completed steps and the once-only activation are not
replayed. Another action may pause the same run again. The variables/context
remain GM-only; SQLite's existing runtime projection persists them.

Example steps after an interactive spell:

```json
[
  {
    "when": "@action.completed == 1",
    "action": { "type": "chat", "text": "The spell completes.", "gm_only": false }
  },
  {
    "when": "@action.countered == 1",
    "action": { "type": "chat", "text": "The spell is interrupted.", "gm_only": false }
  }
]
```

Continuation effects do not recursively fire other triggers. This does not alter
old manual roll-request steps. See ADR-0042 and ADR-0043.

## Spatial requirements (protocol 17)

A plugin action may declare:

```json
{
  "spatial": {
    "feature": "spatial_rules",
    "sight": false,
    "line_of_effect": true,
    "area_line_of_effect": true
  }
}
```

All flags default to false; `feature` is optional and references a campaign
feature. `sight` tests source to target or area origin against opaque zones.
`line_of_effect` tests that path against solid zones. For area actions only,
`area_line_of_effect` filters each target against solid zones from the area origin.
The same action requirements govern reaction eligibility and execution. The core
checks paths before costs; deferred actions recheck them on resolution.

GM/co-GM can draw ordinary zones and set independent `occlusion.sight` and
`occlusion.effect` flags. Existing zones default to neither. Zones remain private;
clients only show candidate area targets and the core decides results. See
[ADR-0044](adr/0044-declarative-spatial-requirements.md) for the conservative 2D
center-ray model and limitations, including senses, partial cover and lighting.
