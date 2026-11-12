# LESS-rs veritabanı sürücü protokolü

Her sürücü ayrı bir yürütülebilir (`less-driver-<ad>`), stdio üzerinde **satır ayraçlı JSON-RPC 2.0** konuşur
(her satır tek JSON nesnesi). Kaynak: `crates/less-db/src/protocol.rs`, sunucu iskeleti `less-db::serve`
(`Driver` trait'i + `run_stdio`), istemci `less-db::DriverClient`.

## Yaşam döngüsü

1. İstemci ikiliyi başlatır, `connect{config}` gönderir (ilk istek; öncesinde diğer yöntemler `-32001` döner).
2. Her istek `{"jsonrpc":"2.0","id":N,"method":…,"params":…}`; yanıt `{"jsonrpc":"2.0","id":N,"result":…}` ya da
   `{"id":N,"error":{"code":…,"message":…}}`. Yanıtlar istekler sırayla işlendiği için sırayla gelir; **`cancel`** istisna:
   sorgu sürerken de hemen yanıtlanır ve çalışan sorguyu keser.
3. `close` → `null` yanıtı, süreç çıkar. stdin kapanırsa süreç çıkar. Sürücü çökerse istemci `DriverError::Crashed`
   (stderr'in son satırlarıyla) döner, editör düşmez.

## Yöntemler

| Yöntem | Parametreler | Sonuç |
|---|---|---|
| `connect` | `{config: Connection}` (connections.json öğesi: `name,type,host,port,user,password,database,options,file,url`) | `{server_version?}` |
| `list_schemas` | – | `string[]` |
| `list_tables` | `{schema?}` | `[{schema?, name, kind: "table"\|"view"}]` |
| `columns` | `{table}` (`şema.tablo`) | `[{name, data_type, nullable, default?, primary_key}]` |
| `primary_key` | `{table}` | `string[]` (anahtar sırasıyla) |
| `query` | `{sql, read_only, limit?}` | `{columns, rows, affected?, notice?}` |
| `table_data` | `{table, limit, offset}` | `{columns, rows}` (salt okunur) |
| `transaction` | `{statements: string[]}` | `{affected}` (tek transaction; hata → hiçbiri uygulanmaz) |
| `cancel` | – | `null` |
| `close` | – | `null` |

- `rows` değerleri `string | null` (null = NULL). Sayılar metne çevrilir; SQLite blob'u `<blob N bytes>`.
- Birden çok ifadeli `query`'de son sonuç kümesi döner; satır döndürmeyen ifadelerin etkilenen satırı `affected`'ta toplanır.
- `limit` aşılırsa fazla satırlar atılır, `notice` "Showing the first N rows" olur.
- **Salt okunur oturum (#68):** `read_only = true` iken sürücü oturumu salt okunur yapar (SQLite `PRAGMA query_only = ON`,
  PostgreSQL `SET default_transaction_read_only = on`); `false` iken geri açar. Okuma sorgusu olduğuna karar veren
  `less_db::classifier::is_read`'dir; sürücü yalnız ikinci savunma hattıdır.
- Hata kodları: `-32700` ayrıştırma, `-32601` bilinmeyen yöntem, `-32602` geçersiz parametre, `-32000` veritabanı
  hatası (mesaj kullanıcıya gösterilir), `-32001` bağlı değil.
- Zaman aşımları (istemci): sorgu / `table_data` / `transaction` 5 dk, diğerleri 20 sn.

## Sürücü yöneticisi (`less_db::manager`)

- Arama sırası: çalışan exe'nin klasörü (`cargo run` geliştirme derlemesi; `deps/` ise bir üstü), sonra
  `~/.local/share/less-rs/drivers/` (`LESS_RS_DATA_DIR` ile değişir).
- `plan_install(type)`: `https://github.com/oguzmuay/less-editor/releases/download/drivers-v<ver>/manifest.json`
  dosyasını (küçük JSON) çeker, `InstallPlan{binary,url,sha256,dest}` döner. Manifest biçimi:
  `{"version":"0.1.0","drivers":{"aarch64-apple-darwin":{"less-driver-postgres":{"url":"…","sha256":"…"}}}}`.
- `install(plan, approved)`: `approved == false` → hiçbir ağ isteği yapmadan `NotApproved`. Onaylıysa indirir, SHA-256'yı
  doğrular (uyuşmazsa dosya atılır), `chmod 755`, `.part` → atomik `rename`. `file://` URL'leri de çalışır (test).
- İkili adları: `less-driver-postgres`, `-sqlite`, `-mysql` (MariaDB de), `-sqlserver`, `-redis`, `-mongodb`.

## Bilinen eksikler

- Yazılmayan sürücüler: **MySQL/MariaDB, SQL Server, Redis, MongoDB** (protokol onları da kapsar; `DbType::driver_binary`
  adları hazır). Redis/MongoDB için `list_tables` anahtar/koleksiyon, `query` komut/betik anlamına gelmeli.
- PostgreSQL sürücüsünde TLS yok (`NoTls`); `sslmode=require` isteyen sunucular bağlanamaz.
- PostgreSQL'de salt okunur oturum `default_transaction_read_only`; sorgu kendisi `SET` ile kapatabilir, bu yüzden
  sınıflandırıcı (`set` yazma sayılır) zorunlu önkoşul.
- Sürücü release iş akışı (`drivers-v*` etiketi, manifest üretimi) henüz yok; `plan_install` gerçek ağda denenmedi.
- Sürücü ikilileri `app` paketine/`cargo run` yanına otomatik konmaz; `cargo build -p less-driver-sqlite` ile aynı
  `target/<profil>/` klasörüne düşer, bulucu orayı bakar.
