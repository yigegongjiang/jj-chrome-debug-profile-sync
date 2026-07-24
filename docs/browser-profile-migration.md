# Chromium Profile 迁移

跨产品迁移 = 文件镜像 + OSCrypt 重加密 + `Secure Preferences` 重签 + 隔离启动验证；缺一项都可能在首次启动后静默丢数据。

## 适用边界

- 同产品 + 同机器 + 同一 Safe Storage：关闭浏览器后可镜像；本项目 Chrome → debug Chrome 属于此类。
- 跨产品：即使都基于 Chromium，也 MUST 重加密目标密文；迁移扩展时 MUST 重签受保护偏好。
- 跨机器：除重加密外，`Secure Preferences` 的 device ID 也变更，MUST 重签。
- 新版本 profile → 旧版本浏览器：MUST NOT 直接迁移；Chrome 官方明确 profile 不向后兼容。
- 跨 OS：MUST 按目标 OS 的 OSCrypt 实现重新设计；本文算法仅覆盖 macOS Chromium `v10`。

## 数据分类

<!-- prettier-ignore -->
| 类别 | 典型文件 | 处理 |
|---|---|---|
| 可直接复制 | `History` / `Favicons` / `Top Sites` / `Shortcuts` / `Bookmarks` / `Sessions` / 站点存储 | 校验 schema 后复制 |
| 必须重加密 | `Cookies` / `Extension Cookies` / `Login Data` / `Web Data` 中的加密 BLOB | 源密钥解密 → 目标密钥加密 |
| 必须重签 | `Secure Preferences` | 使用目标产品 seed + 当前机器 ID 重算 |
| 保留目标外壳 | User Data 根 `Local State` + `Preferences.profile` 身份字段 | 保持目标 profile 目录映射 / 名称 / 管理属性 |
| 产品私有 | 浏览器专属 agent / assistant / workspace DB | 目标无消费者时排除 |
| 可重建 | Cache / GPU / Shader / Crashpad / Metrics / 单例锁 | 排除 |

扩展若迁移，MUST 同时复制：

- `Extensions`
- `Extension State` / `Extension Rules` / `Extension Scripts`
- `Local Extension Settings` / `Sync Extension Settings` / `Managed Extension Settings`
- `Preferences` / `Secure Preferences` 中的扩展注册

扩展若不迁移，MUST 同时移除目录、注册与对应 protection hash；只删 `Extensions/` 会触发重装或残留。

## 流程

### 1. Preflight

1. 解析源/目标 App 的真实 Chromium framework 版本；不能只比较产品版本。
2. 检查关键 SQLite `meta.version` / `meta.last_compatible_version`。
3. 读取目标 `Local State.profile.info_cache`，固定 profile 目录映射。
4. 记录源数据基线：表记录数、扩展 ID、站点存储字节数、关键文件 SHA-256。
5. 优雅退出全部源/目标浏览器；确认 helper / renderer / extension 进程均消失。
6. 对源数据库执行 `PRAGMA integrity_check`。

```bash
ps ax -o pid=,command= | rg '/Applications/(Dia|Google Chrome)\.app/'
sqlite3 "$PROFILE/History" \
  "SELECT key,value FROM meta WHERE key IN ('version','last_compatible_version');"
sqlite3 "$PROFILE/History" 'PRAGMA integrity_check;'
```

版本号只是快速门槛；最终兼容性以每个数据库 schema + 目标浏览器隔离启动为准。

### 2. Stage

1. 在目标 User Data 同卷创建临时目录，保证最终 `rename(2)` 原子替换。
2. 源 profile → 单份 base stage；排除产品私有数据、缓存、锁文件。
3. 在 base stage 完成全部转换 + 静态校验。
4. base stage → 每个目标 profile stage；只补回各目标的 profile 外壳字段。
5. 禁止在校验完成前启动真实目标 profile。

推荐 `rsync -aE` 保留 macOS 扩展属性；大目录复制可用 APFS clone，最终结果不能依赖 clone。

### 3. OSCrypt 重加密

本次验证过的 macOS Chromium `v10`：

```text
key = PBKDF2-HMAC-SHA1(
  password = Keychain Safe Storage password,
  salt = "saltysalt",
  iterations = 1003,
  dkLen = 16
)

blob = "v10" || AES-128-CBC-PKCS7(
  key = key,
  iv = 16 × 0x20,
  plaintext
)
```

处理规则：

- 分别从源/目标产品的 Keychain service + account 读取 Safe Storage password。
- password / derived key MUST 只存在内存；NEVER 输出到 stdout / report / 临时文件。
- 先枚举已知字段，再扫描 SQLite BLOB 的 `v10` 候选；候选必须能被源密钥解密才可转换。
- 当前实测字段：
  - `Cookies.cookies.encrypted_value`
  - `Extension Cookies.cookies.encrypted_value`
  - `Login Data.logins.password_value`
  - `Web Data.credit_cards.card_number_encrypted`
  - `Web Data.local_stored_cvc.value_encrypted`
  - `Web Data.keywords.url_hash`
- 每个数据库单事务更新；commit 后 `PRAGMA wal_checkpoint(TRUNCATE)` + `PRAGMA integrity_check`。
- 转换计数 MUST 同时等于源密钥可解密数、目标密钥可解密数。

Cookie schema v24 的明文为：

```text
SHA256(host_key) || cookie_value
```

重加密时保留整段明文；验证时 MUST 检查前 32 bytes 等于 `SHA256(host_key)`。

### 4. `Secure Preferences` 重签

直接复制源文件会携带源产品签名；Chrome 首次启动可能删除扩展注册。删除 `protection` 也不能保留扩展。

输入：

- `seed`：目标 Chrome 构建内的 `IDR_PREF_HASH_SEED_BIN`。
- `device_id`：macOS `IOPlatformUUID`。
- `target_key`：目标 OSCrypt key。
- `value`：`Secure Preferences` 对应 preference value。

seed 获取：

1. 有对应构建源码/生成资源映射时，直接解析 `IDR_PREF_HASH_SEED_BIN`。
2. 仅有已安装 App 时，解析 `resources.pak` DataPack v4/v5；用目标 profile 已知有效的 `protection.super_mac` 验证候选。
3. 候选 MUST 唯一；resource ID / seed 内容随构建变化，MUST NOT 硬编码。

序列化 `ValueAsString(value)` MUST 与 Chromium 完全一致：

- 缺失值 → 空字符串。
- dict 深拷贝后递归移除空 dict / list。
- 紧凑 JSON + Chromium 键顺序；Python 实测使用 `sort_keys=True`。
- `<` / U+2028 / U+2029 分别转义为 `\u003C` / `\u2028` / `\u2029`。

签名：

```text
legacy_mac =
  HEX_UPPER(HMAC-SHA256(seed, device_id || path || ValueAsString(value)))

encrypted_hash =
  Base64(OSCrypt_target(
    SHA256(seed || path || ValueAsString(value))
  ))
```

`extensions.settings` 是 split preference：每个扩展使用 `extensions.settings.<extension_id>` 单独计算。存储键后缀 `_encrypted_hash` 不属于实际 preference path。

汇总签名：

```text
super_mac = legacy_mac(path = "", value = protection.macs)

super_encrypted_hash = encrypted_hash(
  path = "",
  value = recursively_keep_only_keys_ending_in("_encrypted_hash")
)
```

所有 leaf MAC / encrypted hash + 两个汇总签名 MUST 反向验证。

### 5. Profile 外壳

- User Data 根 `Local State` SHOULD 保留目标版本；它定义 profile 目录、缓存名、全局设置。
- 每个 stage 从旧目标保留最小身份字段：名称、头像、创建时间、managed 属性、`enterprise_profile_guid`。
- 其余 profile 数据来自源。
- 需恢复 session cookie / tabs 时，`Preferences.profile.exit_type="Crashed"` + `exited_cleanly=false`；是否采用由迁移目标决定。
- Chrome Sync 是独立数据源。要求精确本地镜像时，smoke test MUST `--disable-sync`；正式启动若开启 Sync，服务端数据仍可能重新合并。

### 6. 静态验证

替换前 MUST 全部通过：

- `Cookies` / `Extension Cookies` / `Favicons` / `History` / `Login Data` / `Web Data`：`PRAGMA integrity_check = ok`。
- 所有目标 `v10` BLOB 可由目标 key 解密；Cookie host hash 全部正确。
- 所有 `Secure Preferences` leaf / super 签名正确。
- stage 与源的 History / passwords / downloads / bookmarks / extensions / site storage 指标符合策略。
- 多目标 stage 的迁移数据 SHA-256 一致；仅允许 profile 外壳字段不同。
- 无源产品私有文件、锁文件、WAL 残留、迁移 secret。

报告 MUST 记录：

- 源/目标产品 + Chromium 版本
- profile 映射 + 排除规则
- 源/stage/首次启动后指标
- 每个重加密字段的记录数
- 重签/反向验证数量
- SQLite integrity 结果
- smoke test 结果
- 是否保留旧目标；NEVER 记录密钥

### 7. 隔离启动

只启动 stage clone：

```bash
"$CHROME_BIN" \
  --headless=new \
  --user-data-dir="$SMOKE_USER_DATA" \
  --profile-directory=Default \
  --no-first-run \
  --no-default-browser-check \
  --disable-background-networking \
  --disable-component-update \
  --disable-sync \
  --remote-debugging-port=0 \
  about:blank
```

验证器 MUST：

1. 等待 `$SMOKE_USER_DATA/DevToolsActivePort`，不能等待浏览器自行退出。
2. ready 后留 2–5 秒让 profile 初始化。
3. 先 terminate + 限时等待，超时才 kill。
4. 再做数据库/密文/签名/指标验证。

首次启动允许：

- 清理已过期 persistent cookies。
- History visits 增加。
- Sessions 文件数变化。
- 站点存储产生少量运行态变化。

因此 Cookie 门槛是“未过期 persistent cookies 零损失”，不是总数不变。Chromium 时间：

```text
chrome_time_us = unix_time_us + 11644473600000000
```

扩展迁移模式下，源扩展目录 ID MUST 全部仍存在于启动后的 `Secure Preferences.extensions.settings`。

### 8. 原子替换

1. 为所有目标准备完 stage。
2. `target → transient-old`。
3. `prepared → target`。
4. 对真实目标重复静态验证。
5. 任一失败：反向 rename 回滚全部已替换目标。
6. 全部成功且明确不保留备份：删除 `transient-old` + stage。

临时旧目录是事务回滚状态，不是长期备份；删除前 MUST 已完成最终验证。

## 常见失败

- 直接复制加密 DB：SQLite/记录数正常，但 Cookie / password / card 无法解密。
- 只删 `Secure Preferences.protection`：Chrome 初始化 protection 时丢弃源扩展注册。
- 只复制扩展目录：扩展未注册；或配置残留导致 Web Store 静默重装。
- 用 `--dump-dom` 等待 smoke test 退出：extension/service-worker 进程可令 headless 长时间不退出。
- 以 Cookie 总数比较首次启动：过期 Cookie 正常清理会造成误报。
- 浏览器运行时复制：SQLite WAL / JSON 写入竞态，得到非一致快照。
- 先覆盖再测试：Chrome 首次启动会改写证据，失败后难以区分迁移错误与启动清理。
- 硬编码 pref seed resource ID：Chrome 更新后静默签错。

## 2026-07-24 Dia → Chrome 实证

- Dia Chromium `150.0.7871.182` → Chrome `150.0.7871.186`；关键 SQLite schema 完全匹配。
- 源 `Default` → Chrome `Default` + `Profile 8`；保留两个目标 profile 外壳，不保留旧 Chrome 数据。
- 源基线：122,855 URLs / 1,977,548 visits / 2,829 cookies / 1,141 passwords / 7 extensions / 约 4.43 GB site storage。
- 重加密 4,013 个 BLOB；重签并验证 38 legacy MAC + 38 encrypted hash。
- Dia-only agent / assistant / workspace 数据排除。
- 隔离启动先后发现并修正：
  - `--dump-dom` 假超时。
  - 过期 Cookie 清理误判。
  - 扩展注册因源产品签名被 Chrome 删除。
- 所有问题均在 stage clone 暴露；真实目标只在全部门槛通过后原子替换。
- 最终两个目标数据库 integrity 全部 `ok`、迁移数据一致、无迁移临时目录、无长期备份。

## 官方依据

- [Chrome profile version compatibility](https://support.google.com/chrome/a/answer/9866158?hl=en)
- [Chromium macOS OSCrypt](https://chromium.googlesource.com/chromium/src/+/38c29b6535f88af0bbe843e0416390018d965da6/components/os_crypt/sync/os_crypt_mac.mm)
- [Chromium cookie store schema / host hash](https://chromium.googlesource.com/chromium/src/+/refs/heads/main/net/extras/sqlite/sqlite_persistent_cookie_store.cc)
- [PrefHashCalculator](https://chromium.googlesource.com/chromium/src/+/HEAD/services/preferences/tracked/pref_hash_calculator.cc)
- [PrefHashStore implementation](https://chromium.googlesource.com/chromium/src/+/HEAD/services/preferences/tracked/pref_hash_store_impl.cc)
- [macOS device ID](https://chromium.googlesource.com/chromium/src/+/HEAD/services/preferences/tracked/device_id_mac.cc)
- [Chrome pref hash seed loading](https://chromium.googlesource.com/chromium/src/+/refs/heads/main/chrome/browser/prefs/chrome_pref_service_factory.cc)
- [Chromium DataPack](https://chromium.googlesource.com/chromium/src/+/refs/heads/main/ui/base/resource/data_pack.cc)
- [Chromium JSON escaping](https://chromium.googlesource.com/chromium/src/+/refs/heads/main/base/json/string_escape.cc)
