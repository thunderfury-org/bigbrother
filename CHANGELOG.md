# Changelog
## [0.3.0] - 2026-08-22
### Bug Fixes

- (app) paginate pan123 share file listing (#157)
- (app) cut releases through a PR before tagging (#154)

### Features

- (app) transfer TV episodes concurrently within a season (#156)
- (app) search pan1 community threads from the console (#155)
## [0.2.0] - 2026-08-21
### Bug Fixes

- (app) paginate library listing and cache directory ids (#150)
- (app) show this-run import history instead of season gaps (#147)
- (app) upgrade action-gh-release to v3 for Node 24 (#133)
- (app) stop automatic docker builds on main (#134)
- (app) restrict docker latest tag to stable version tags only (#128)

### Chores

- (app) release v0.2.0 (#153)

### Documentation

- (app) move release workflow into agent docs and slim README (#152)
- (app) tighten AGENTS.md and satellite pointers (#135)

### Features

- (app) persist TMDB catalog responses in sqlite cache (#151)
- (app) batch subscription File Index rescan into one import (#146)
- (app) enrich subscription console with TMDB posters and year (#143)
- (app) restyle web console as a light workbench (#142)
- (app) add media directory browse and delete to web console (#141)

### Refactoring

- (app) implement DownloadUrlCache on Cache (#149)
- (app) fold download url source into pan library gateway (#148)
- (app) simplify application ports (#140)
- (app) collapse application service type parameters (#139)
- (app) collect hexagonal ports and composition root (#138)
- (app) collapse workspace into a single src crate (#137)
- (app) sink media source observation processing into application (#136)

### Support

- (app) bump console and sea-orm dependencies (#144)
## [0.1.0] - 2026-07-02
### Bug Fixes

- (app) update 123pan web API base to yun.123pan.com (#125)
- (issue-103) deepen download URL resolution seam
- (app) filter trashed files from pan123 list and search results (#117)
- (app) tolerate missing pan189 share file arrays (#107)
- (app) rate limit pan115 share requests (#106)
- (app) classify pan189 ShareNotFound as permanent failure (closes #92) (#94)
- (app) classify telegram import permanent failures (closes #88) (#90)
- (app) include show name in tv import notifications without success (#83) (#84)
- (app) skip quark batch lookup for empty share (#80)
- (app) exclude source tags from tv scene titles (#79)
- (app) continue tv title fallback after cached miss (#74)
- (app) update dependency version (#72)
- (app) filter unsupported telegram links before import (#65)

### Chores

- (app) release v0.1.0

### Documentation

- (app) integrate Karpathy agent guidelines into AGENTS.md (#118)

### Features

- (app) add formal release process with changelog and CI automation (#126)
- (app) subscription as sole import gate (#121) (#123)
- (app) import media from file index via HTTP console (#119)
- (app) migrate pan123 client from web API to OpenAPI (#116)
- (app) add OpenAPI refresh token auth for pan123 SHA1 fast upload (#115)
- (app) cinematic movie-poster redesign for web console (#114)
- (app) use openapi base URL for pan123 SHA1 fast upload (#113)
- (app) remove quark cloud drive support (#112)
- (app) LLM-based title extraction as regex parser fallback (#109)
- (app) index only media files in file index (#105)
- (app) migrate console to Svelte 5 SPA (closes #91) (#93)
- (app) add console with import history and file index search (closes #87) (#89)
- (app) add import history console with recorded import pipeline (closes #85) (#86)
- (app) add telegram source observability (#77)
- (app) add telegram export file-index CLI (#69)
- (app) add reusable share resolver for CLI and Telegram import flows (#61)
- (app) add Trellis project infrastructure and coding guidelines (#56)
- (app) parser 剧集识别 & 导入失败报告 (#51)
- (app) support quark cloud drive share import (#50)

### Refactoring

- (app) split media identify from importer (#120) (#122)
- (app) simplify message sender seam (#102)
- (app) token pipeline parser (issue #81) (#82)
- (app) simplify file index fingerprint model (#66)
- (app) move bootstrap/config/logger into interface/cli (#59)
- (app) split AppError into granular variants with retryable semantics (#58)
- (app) split RequestError into fine-grained variants (#57)
- (app) simplify CLI entry point and remove bootstrap/services (#55)
