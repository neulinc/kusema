# Running a private SearXNG server for Kusema

[SearXNG](https://docs.searxng.org/) is a free, open-source metasearch engine. It sends your searches to public search engines on your behalf and returns the results, without accounts, tracking or ads. A firm can run its own server so that Kusema's web searches go through a machine the firm controls, instead of a search company's API.

This guide covers a small, private server for one office: Docker, basic hardening, keeping it private, and pointing Kusema at it. It follows SearXNG's own documentation as of November 2026; check it for anything newer.

## What a SearXNG server does and doesn't do

- **It sees every search.** Kusema sends the search text (never your question's screenshot or selected text). Whoever runs the server can log it, so run it yourself or with someone you trust.
- **It passes searches on.** The search engines SearXNG asks (Google, Bing, DuckDuckGo, Brave and others, as you configure) see searches coming from your server's address, not from each person's computer, and get no cookies or account.
- **It isn't a firewall.** Kusema only ever sends search text to it. Links Kusema reads are fetched by Kusema directly, not through SearXNG.

## Requirements

- A small Linux server or virtual machine: 1–2 CPU cores, 1–2 GB of RAM. An office server, a NAS that runs Docker, or a cloud VM all work.
- [Docker](https://docs.docker.com/engine/install/) with the Compose plugin.
- If people use Kusema away from the office: a domain name and a TLS certificate (see _Keeping it private_).

## 1. Install with Docker Compose

SearXNG's recommended way to run it in containers is Docker Compose ([installation docs](https://docs.searxng.org/admin/installation-docker.html)). The older `searxng-docker` repository was archived in March 2026; use the files from the main repository instead.

```bash
mkdir searxng && cd searxng
curl -fsSL -O https://raw.githubusercontent.com/searxng/searxng/master/container/docker-compose.yml \
           -O https://raw.githubusercontent.com/searxng/searxng/master/container/.env.example
cp -i .env.example .env
```

Read `.env` and set at least the address people will use, for example `SEARXNG_BASE_URL=https://search.yourfirm.example/` (variables named `SEARXNG_*` map to SearXNG's settings, see `.env.example`). Then start it:

```bash
docker compose up -d
```

The container listens on port 8080. Its settings live in `core-config/settings.yml` (mounted at `/etc/searxng` inside the container), and its cache in `/var/cache/searxng`.

## 2. Turn on JSON results

Kusema asks for results as JSON (`/search?q=…&format=json`). SearXNG answers **403 Forbidden** to formats that aren't turned on, and many public instances leave JSON off ([search API docs](https://docs.searxng.org/dev/search_api.html)). In `core-config/settings.yml`, under `search:`, list `json` as well as `html`:

```yaml
search:
  formats:
    - html
    - json
```

Restart with `docker compose restart`.

## 3. Basic hardening

In `core-config/settings.yml` ([server settings](https://docs.searxng.org/admin/settings/settings_server.html)):

- **`server.secret_key`**: set a long random value (for example the output of `openssl rand -hex 32`). The example file's value must be changed. The environment variable `SEARXNG_SECRET` sets it too.
- **`server.base_url`**: the address people use (`SEARXNG_BASE_URL`).
- **`server.public_instance: false`** (the default) for a private server.
- **`server.image_proxy`**: leave off unless you use the web interface's images; Kusema doesn't need it.
- **`server.limiter`**: rate-limits requests and blocks bots. It's meant for public instances and needs a Valkey database ([limiter docs](https://docs.searxng.org/admin/searx.limiter.html)). A private server reachable only from the office can leave it off; a server reachable from the internet should turn it on and set `trusted_proxies` correctly if it sits behind a reverse proxy.
- Keep the image up to date: `docker compose pull && docker compose up -d`, monthly or when SearXNG announces a security fix.
- SearXNG sends `X-Robots-Tag: noindex, nofollow` and `Referrer-Policy: no-referrer` by default; keep them.

## 4. Keeping it private

Pick one:

- **Office network only (simplest).** Don't forward any port from the internet. Use the server's office address, for example `http://192.168.1.20:8080`. Kusema accepts a server on the office network. People away from the office can reach it over the firm's VPN.
- **Reachable from anywhere.** Put it behind a reverse proxy (Caddy, nginx or Apache) that adds TLS, so the address starts with `https://`, and restrict who can reach it: an IP allow-list, the firm's VPN, or the proxy's authentication if every user can be given the server address with it. Turn on the limiter (above). SearXNG's docs link example nginx and Apache configurations.

Don't publish the address. A private server that the public finds and uses becomes a public search proxy under your name.

## 5. Point Kusema at it

**One person:** in Kusema, open **Assistant → Settings → Web search**, turn on **Use the web**, choose **SearXNG server** under **Search with**, enter the address (for example `https://search.yourfirm.example`), and press **Test**. Kusema runs one search ("Kusema test search") and says how many results came back; it's listed on the **Privacy** page like any other search.

**Everyone at the firm:** set it once for every computer with the Windows policy value `SearxngUrl` (see [IT administration](it-admin.md)). Kusema then uses that server for SearXNG searches, shows it as set by your administrator, and the user can't change it.

## Troubleshooting

| What Kusema says                                | What to check                                                                                                                            |
| ----------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------- |
| "the SearXNG server doesn't allow JSON results" | Step 2: add `json` under `search: formats:` and restart.                                                                                 |
| "couldn't connect" or a timeout                 | The address and port; that the computer can reach the server (VPN, firewall).                                                            |
| No results for everything                       | `docker compose logs` on the server: the engines it asks may be rate-limiting it. Enable fewer, or different, engines in `settings.yml`. |

## License

SearXNG is licensed under the [GNU Affero General Public License v3.0 or later](https://github.com/searxng/searxng/blob/master/LICENSE) (AGPL). Kusema doesn't include any SearXNG code; it only sends searches to a server's HTTP interface.

- **Running it unmodified is fine.** Installing the official image and changing only its settings (`settings.yml`, `.env`) is ordinary use.
- **If you modify SearXNG's code on a hosted instance, you must publish your modifications.** The AGPL requires that the people who use a modified instance over a network can get its complete source code, including your changes.

## Run it yourself, or have it done

Firms can run SearXNG themselves with this guide. If you'd rather not, Neulinc can set up and maintain a private SearXNG server for your firm: [neulinc.com/kusema](https://neulinc.com/kusema).
