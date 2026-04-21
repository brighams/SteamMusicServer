We have a new crate steam-vent.

We need an async module that runs when the scan is complete.
We do not need to wait for it. It will be a database writer, everyone else is a reader, should be fine.

1st we need new table generated at scan time, with 1 row per title:

appid  
error-string (null if no errors, otherwise diagnostic info here, rest is null)
public-capsule-hash 
public-hero-hash 
public-lbrary-hash 

We will worry about cache refresh later, we really don't need to update unless:
cache-url is empty
cache-file-path does not exist

async logo loader outline:  [[ this is rough, it depends on how things check out with steam-vent, some research from online claude follows below]]

 At start/table creation time, create new table in player database if not exist call it logo_cache
 create directory in media/logo_cache
 at scan time, if not exist insert an appid and otherwise blank row (primary key = appid)

 AFTER scanning, kick off async thread and continue with startup
 
-- conduct steam-vent logon [see below for details]
loop
  # this is either in chunks [confirm size], or 1 by 1, but maintain the same login session and connection if possible!
  select 200 rows from logo_cache where error-string is null and public-url is null
  
  batch these 200 appids into an API request using steam-vent the english library image  [See below for steam-vent info]
  process the results of the API call back into the database, computing the cache-file-path
endloop

-- logoff


That's IT, let's get this working with the API calls

------ notes from research with online claude ---------

See steam-vent source code in project!
crate has already been added, source is temporary for reference only.

# Steam Library Art Fetching — Implementation Notes

Handoff notes for implementing Steam library artwork fetching (game covers + OST
album covers) in a Rust music player, using the `steam-vent` crate.


## Data flow

```
appids (Vec<u32>)
    │
    ▼
┌──────────────────────────────────┐
│ steam-vent: Connection::anonymous│   one logon, reused for all batches
└──────────────────────────────────┘
    │
    ▼  chunks of ~200 appids
┌──────────────────────────────────┐
│ CMsgClientPICSProductInfoRequest │   batched PICS call
└──────────────────────────────────┘
    │
    ▼  CMsgClientPICSProductInfoResponse (vdf/KeyValue blob per app)
┌──────────────────────────────────┐
│ parse `common.library_assets_full│
│   .library_capsule.image2x.<lang>│
│   .library_hero.image.<lang>     │
│   .library_logo.image2x.<lang>   │  
└──────────────────────────────────┘
    │
    ▼  asset hash strings
┌──────────────────────────────────┐
│ URL: https://steamcdn-a.akamaihd │
│   .net/steam/apps/{appid}/{hash} │
│   .jpg                           │
└──────────────────────────────────┘
    │
    ▼
HashMap<u32, LibraryAssets>
```

Notes on the KeyValue structure:
- Some apps have `library_assets_full`, others have the older `library_assets`
  — handle both, prefer `_full`.
- Variants: `image` (1x) and `image2x` (2x resolution). Prefer `image2x`, fall
  back to `image`.
- Per-language keys: `english`, `schinese`, `japanese`, etc. Default to
  `english`, fall back to whatever is present.
- `library_logo` may also contain a `logo_position` subkey that is **metadata,
  not an image** — skip it.
- Not every app has all three asset types. OSTs in particular usually only
  have `library_capsule`.

---

## Proposed Rust API

```rust
use std::collections::HashMap;

#[derive(Debug, Clone, Default)]
pub struct LibraryAssets {
    pub capsule_url: Option<String>,  // the 2:3 game cover / square OST cover
    pub hero_url: Option<String>,     // wide background, games only usually
    pub logo_url: Option<String>,     // transparent PNG logotype, games only
}

pub async fn fetch_library_assets(
    appids: &[u32],
) -> Result<HashMap<u32, LibraryAssets>, FetchError>;
```

Implementation sketch (pseudocode — real calls depend on what `steam-vent`
actually exposes for PICS):

```rust
pub async fn fetch_library_assets(
    appids: &[u32],
) -> Result<HashMap<u32, LibraryAssets>, FetchError> {
    let server_list = ServerList::discover().await?;
    let mut conn = Connection::anonymous(server_list).await?;

    let mut out = HashMap::new();
    for chunk in appids.chunks(200) {
        let mut req = CMsgClientPICSProductInfoRequest::new();
        for &id in chunk {
            let mut app = CMsgClientPICSProductInfoRequest_AppInfo::new();
            app.set_appid(id);
            req.apps.push(app);
        }
        // if steam-vent exposes a typed call, prefer it; otherwise:
        let resp: CMsgClientPICSProductInfoResponse =
            conn.send_recv(req).await?;

        for app in resp.apps {
            let kv = parse_vdf(&app.buffer)?;           // binary KeyValue
            let assets = extract_library_assets(app.appid(), &kv);
            out.insert(app.appid(), assets);
        }
    }
    Ok(out)
}

fn extract_library_assets(appid: u32, kv: &KeyValue) -> LibraryAssets {
    let common = kv.get("common");
    let full   = common.and_then(|c| c.get("library_assets_full"));

    LibraryAssets {
        capsule_url: pick_asset(appid, full, "library_capsule"),
        hero_url:    pick_asset(appid, full, "library_hero"),
        logo_url:    pick_asset(appid, full, "library_logo"),
    }
}

fn pick_asset(appid: u32, full: Option<&KeyValue>, kind: &str) -> Option<String> {
    let node = full?.get(kind)?;
    // prefer image2x, fall back to image
    let variant = node.get("image2x").or_else(|| node.get("image"))?;
    // prefer english, fall back to first language present
    let hash = variant.get("english")
        .or_else(|| variant.children().next())?
        .as_str()?;
    // skip metadata entries like logo_position
    if hash.contains('/') || hash.len() < 10 { return None; }
    Some(format!(
        "https://steamcdn-a.akamaihd.net/steam/apps/{}/{}.jpg",
        appid, hash
    ))
}
```

---

## Rate limiting / scale notes

- Valve throttles **sessions / logons**, not individual PICS requests within
  a session. One connection, many batched calls = safe. Spawning N connections
  for N appids = gets you `AccountLoginDeniedThrottle`.
- No published batch-size limit. 200 appids per call is a safe conservative
  chunk. DepotDownloader and SteamDB go higher in practice.
- Target scale: 1200 games + 300 OSTs = ~1500 appids = ~8 chunks = one
  session, seconds of work.
- **Cache the URLs.** Asset hashes change rarely. Persist a
  `HashMap<appid, LibraryAssets>` to disk and only re-fetch when the user
  requests a refresh or when `PICSGetChangesSince` says something moved. Do
  not re-fetch on every app launch.

---

## Failure modes to handle

| Failure | Cause | Handling |
|---|---|---|
| `app.only_public = false` / no `common` section | App not public or delisted | Skip, log appid, return empty assets |
| Missing `library_assets_full` | Older app never migrated | Try old paths: `steam/apps/{appid}/library_600x900.jpg` etc. — these still work for pre-2020-ish games |
| `library_capsule` present but `library_hero`/`logo` absent | Common for OSTs, DLC, small indies | Return `None` for missing ones, don't error |
| `AccountLoginDeniedThrottle` on logon | Too many recent connection attempts | Exponential backoff, retry after 5–30 min |
| Connection drops mid-session | Steam CM cycling | Reconnect once, resume from next chunk |

---

## Test fixtures (capture these during POC)

Run SteamFetch against these and save the output — they become Rust test
assertions:

```
881100     Noita (game) — should have capsule + hero + logo
1161100    Noita OST (verify actual appid on steamdb.info first)
440        Team Fortress 2 (every asset type, multiple languages — good edge case)
730        CS2 (same — high-traffic app, expect all assets populated)
```

For each, record:
1. Exact URLs returned
2. Presence/absence of each asset type
3. Which language keys exist

---

## What NOT to do

- Do **not** compute URLs from appid alone (e.g. `steam/apps/{appid}/library_600x900.jpg`).
  That pattern is legacy and only works for older games.
- Do **not** hit `store.steampowered.com/api/appdetails` for library art — it
  doesn't return the new hashed URLs.
- Do **not** make one `steam-vent` connection per appid. Batch them.
- Do **not** assume OSTs have a separate `album_cover` type. They reuse
  `library_capsule`. Aspect ratio is a property of the image, not a property
  of the asset slot.
- Do **not** ship with SteamKit2/.NET as a runtime dependency if `steam-vent`
  can do the job natively. Sidecar-SteamFetch is only a fallback if PICS turns
  out to be genuinely painful in `steam-vent`.

---

## Open questions to resolve on day one

1. Does `steam-vent` expose `PICSGetProductInfo` at a typed-API level, or do
   we send `CMsgClientPICSProductInfoRequest` raw? (Check source + docs.rs.)
2. What VDF/KeyValue parser does `steam-vent` ship with? If none, pull in
   a standalone crate (`keyvalues-parser`, `vdf-reader`, or similar).
3. Is the PICS response buffer binary VDF or text VDF? (Historically binary
   over the wire — confirm by inspecting a real response.)
4. What's the real Noita OST appid? (Look up on steamdb.info before writing
   fixtures.)
