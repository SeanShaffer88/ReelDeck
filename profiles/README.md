# Output profiles

Saved output settings for `reeldeck convert --settings <file>`. Each file
is one `OutputSettings` record: container, video and audio codec, and
optionally size, frame rate and bitrates (`null` keeps the source's size
and rate, or uses the codec's quality-based default bitrate). Copy one
and edit it to make a profile for a specific device or platform;
`reeldeck convert` refuses a profile whose combination the container
cannot carry, and says why.

| File | For |
| --- | --- |
| `web-mp4.json` | Sharing and web players: H.264 + AAC in MP4, source size and rate |
| `web-720p.json` | The same at 1280x720, 4 Mbit/s. Scales to exactly 1280x720, so use it for 16:9 sources only |
| `edit-prores.json` | Editing: ProRes 422 HQ + PCM in QuickTime |
| `legacy-avi.json` | Older players: MPEG-4 Part 2 + PCM in AVI |
