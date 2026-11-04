# Content protection (video, audio, images, documents, paid text)

## The honest model
Anything a legitimate viewer can see or hear can be captured — screen recording, HDMI capture, a phone camera. No technique makes that impossible. The goals are therefore, in order:
1. **Zero access without entitlement** — unauthenticated or unpaid users get nothing, not even a hidden copy.
2. **Raise the cost of bulk extraction** for entitled users far above "open DevTools, copy the .m3u8, run a downloader".
3. **Make every leak traceable** to the account that leaked it.
4. **Detect and respond** — spot abnormal consumption and revoke quickly.

Client-side tricks — blocking right-click, detecting DevTools, `debugger` loops, disabling text selection, obfuscating JS — are friction for non-technical users only. They are never counted as a control, and never shipped where they hurt accessibility or UX.

## How DevTools attacks actually work (design against each)
| Attack | What the attacker does | Control |
|---|---|---|
| Network tab → copy MP4/m3u8 URL | Grabs the media URL and downloads it, or shares it | Never serve whole files; private storage; short-lived signed URLs bound to session; segment requests also authorized |
| Read API/JSON payload | Premium content (text, video URL, answers) is in the response and the UI merely hides it | Server returns only entitled fields; check SSR payloads (`__NEXT_DATA__`, RSC stream) for leaks |
| Edit JS / flip `isPremium=true` | Unlocks the UI locally | Entitlement decided server-side on every media/API request; client flags are cosmetic |
| Replay/forge requests | Copies cURL from DevTools, changes IDs | Ownership-scoped queries, signed short-TTL tokens, nonce/replay protection on licence requests |
| HLS AES-128 key grab | Fetches the key URL from the playlist and decrypts offline (yt-dlp, N_m3u8DL-RE) | Key endpoint authorized + token-bound, but treat AES-128 as **light** protection; use DRM for valuable content |
| Credential sharing | One paid account used by many | Concurrent-stream limit, device registration limit, session anomaly detection |
| Scripted scraping | Script iterates all lesson IDs | Rate limits per account on manifests/licences/segments, anomaly alerts, account suspension |

## Layers for video (pick depth by content value — record the choice in `memory/wiki/security/content-protection.md`)
1. **Storage** — originals in a private bucket; only the transcoding pipeline reads them. Public buckets: never.
2. **Packaging** — adaptive streaming (HLS/DASH, CMAF segments). Never a single progressive MP4 for paid content.
3. **Delivery** — CDN signed URLs or signed cookies, TTL in minutes, path-scoped to the one asset, issued by the backend only after an entitlement check. Playback token embeds `user_id`, `asset_id`, `session_id`, `exp`.
4. **Encryption**
   - *Light (low-value or free-for-members content):* HLS AES-128 with an authorized, short-lived key endpoint.
   - *Strong (paid courses, the core business asset):* multi-DRM — **Widevine** (Chrome, Firefox, Android), **FairPlay** (Safari, iOS), **PlayReady** (Edge, Windows) — via CENC/CMAF and a DRM provider (e.g. PallyCon/DoveRunner, EZDRM, Axinom, VdoCipher, or a video platform with built-in DRM such as Mux, Cloudflare Stream, Bunny Stream — verify current DRM support and pricing). The licence server issues a licence only after calling our backend to verify entitlement.
   - Know the limit: Widevine **L3** (desktop browsers) is software-only and has been broken; **L1** (hardware) plus HDCP gives the strongest protection. Premium tiers can require L1/FairPlay for HD and cap L3 at SD.
5. **Watermarking**
   - *Visible dynamic overlay* — the viewer's name/email/ID, semi-transparent, moving position. Cheap; a strong deterrent against screen recording. Render it inside the video pipeline or as a canvas overlay that is re-applied when removed from the DOM.
   - *Forensic (invisible) watermark* — session-unique A/B segment watermarking from the DRM/video provider, so a leaked file identifies the account.
6. **Session controls** — max concurrent streams per account, device limit, heartbeat on playback, revoke all playback tokens on logout/password change/suspension.
7. **Monitoring** — alert on: one account requesting segments faster than real time, many assets in short time, many IPs/countries per account, licence requests without playback heartbeats.

## Images, documents, paid text
- Serve derivatives (resized, watermarked, compressed) — originals never leave the server.
- PDFs/documents: render in a viewer with per-user watermark; download only when the product intends it; signed URL, short TTL.
- Paid text/articles: server renders only what the user is entitled to (preview paragraph for others). The full text never sits in the HTML, JSON, or SSR payload of a non-entitled response.
- Rate-limit listing and detail endpoints; add bot defences where content is the product.

## Legal & process
- Terms of Service forbid redistribution; account suspension policy; takedown workflow for leaked copies (watermark → identify → act).
