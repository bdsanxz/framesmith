# How a native render moves its pictures

Status: draft, for the proof of concept of the native path. Sources: the product owner's order of 2026-10-13 to make decoding and encoding faster, and the order that followed it to do all of it in our own media stack, frx7, with no other program started; the measurements recorded in `PROGRESS.md` under that date; ITU-R BT.709 for the colour arithmetic; `project-file.md` for the settings; the specs of frx7 (`frx7/docs/spec`) for what its parts do.

No other program is started by a render: reading, decoding, encoding, sound and the finished file are all frx7's work, inside the `poc` program. The tests start `ffmpeg` to make their clips and to look into finished files, which is a check from outside.

## 1. The stages

A render has three kinds of work, and they run at the same time:

1. **Footage.** Each video in use has a reader of its own (frx7's paced footage, `frx7/docs/spec/export-pipeline.md`, section 5): a thread that reads the file, decodes it, and brings each picture to the size it is drawn at, two pictures ahead of the frame being drawn and never more. For the CPU renderer the frame loop turns the picture into RGBA and applies the video's colour filter (section 4). The GPU renderer does that on the card.
2. **Frames.** One thread works through the frames in order: it evaluates the timeline, takes the footage pictures the frame needs, and has the renderer draw the frame. The GPU renderer lets two frames be on their way at once: the card draws one while the next is described.
3. **Encoding.** One thread takes the finished frames in order and writes each into frx7's film writer, in the picture format of section 2. The GPU renderer hands its frames over in that format already; the frames of the CPU renderer are converted on this thread. At most one finished frame waits for it.

Rules that hold whatever the timing of the threads:

- **The picture does not depend on timing.** A footage picture is asked for by its number in the source, and the frame waits for exactly that picture. Two renders of one project give the same pictures.
- **Frames reach the encoder in order**, each once. Pictures of footage may be finished out of order by the threads that make them; they are put back in order before a frame takes one.
- **Nothing runs ahead without limit.** A stage that is faster than the next one waits for it, so memory use stays the same for a film of any length.
- **A failure anywhere stops the render** with the message of the stage that failed: a decoder that cannot read its file, an encoder that ends early.
- **A video that is no longer shown** has its decoder stopped two seconds later, with whatever it had read ahead thrown away.
- **A video that is about to be shown** has its decoder started one second of frames before its first frame, at the picture that frame needs, so the frame does not wait for the decoder to start. A render that begins part way through a film looks through its first second for such videos.

## 2. The picture handed to the encoder

The renderer makes frames as RGBA, 8 bits a channel, full range. The encoder is given 8-bit YUV 4:2:0 in planes (`yuv420p`), BT.709, limited range. The CPU renderer's frames are converted in memory, and that arithmetic defines the result. The GPU renderer converts on the card, so that only the planes are read back; its planes agree with the definition to within one level.

With R, G and B from 0 to 255:

```
Y  =  16 + 0.182586 R + 0.614231 G + 0.062007 B
Cb = 128 - 0.100644 R - 0.338572 G + 0.439216 B
Cr = 128 + 0.439216 R - 0.398942 G - 0.040274 B
```

Results are rounded to the nearest whole number. White is (235, 128, 128), black is (16, 128, 128), and a grey has Cb and Cr of exactly 128.

- **Luma** has one sample for each pixel.
- **Chroma** has one sample for each block of two by two pixels. The sample stands at the block's left column, between its two rows, which is where H.264 expects it. It is taken from the two rows of the block with the weights 1, 2, 1 over the column before the block, the block's left column and its right column. At the left edge of the frame the block's own left column stands in for the missing one.
- Alpha is not used: a finished frame is opaque.
- A film whose width or height is odd cannot be 4:2:0. `render` refuses it before the first frame and says that both sides must be even.

The file says what it holds: BT.709 primaries, transfer and matrix, limited range.

## 3. The encoder

Both encoders are the system's, reached through frx7; the film is written by frx7's own MP4 writer.

- `hardware` (the default): H.264 by the machine's own video encoder. Its quality setting follows `render.quality`: `draft` 45, `standard` 65, `high` 80, on the encoder's scale of 1 to 100.
- `software`: H.264 by the system's encoder run on the processor. It takes no quality setting, only an average bit rate, which follows `render.quality`: `draft` 0.06, `standard` 0.13, `high` 0.20 bits for each pixel of each frame, and never fewer than 200 kbit a second for the film. It keeps less of the picture than the hardware encoder and was seen to encode a very small film wrongly (`frx7/docs/spec/os-codecs.md`); it is for a machine whose hardware does not encode the film's size.
- Both write H.264 High profile, or HEVC Main profile when `render.codec` is `hevc`, 8 bits, 4:2:0, with a key picture at least every two seconds and pictures that may be encoded out of the order they are shown in, into an MP4 file whose tables are in front, so that it can start playing before it is fully loaded.
- With `hardware` on a machine whose hardware does not encode the film's size, `render` stops before the first frame and names the settings to change: `codec: hevc` for a film too large for H.264 on hardware, or `encoder: software`. It never falls back by itself.
- HEVC is marked `hvc1` in the file, the mark players of that codec expect.

## 4. Decoding

- **What a reader hands over.** Pictures at the composition's frame rate and at the size the video is drawn at, as 8-bit YUV 4:2:0 in planes, BT.709, limited range: the form of section 2, and for most footage the form it is stored in, so nothing is converted twice. A source that says it is in another matrix or range is brought to this form by frx7's own arithmetic (`frx7/docs/spec/pixel.md`, section 6). A source that does not say is taken to be in this form.
- **What is read.** Files of the MP4 family that hold H.264 or HEVC pictures of 8 or 10 bits. A file of another kind, or pictures in another codec, are refused before the render starts, with the element's line, the file's name and the reason.
- **RGBA is made by us**: in memory for the CPU renderer, on the card for the GPU renderer, which is given the planes as they come. The arithmetic in memory defines the result, and the card agrees with it to within the tolerance the two renderers are held to. Where the GPU renderer has the CPU painter draw a group that holds a video, that video's picture is made in memory for it. With Y, Cb and Cr as above:

  ```
  R = 1.164384 (Y - 16) + 1.792741 (Cr - 128)
  G = 1.164384 (Y - 16) - 0.213249 (Cb - 128) - 0.532909 (Cr - 128)
  B = 1.164384 (Y - 16) + 2.112402 (Cb - 128)
  ```

  Results are rounded and kept within 0 to 255. Alpha is 255.
- **Chroma between its samples.** A chroma sample stands where section 2 puts it. A pixel between samples takes its chroma evenly from the samples around it: across, a pixel in a block's right column lies half way between that block's sample and the next; down, a pixel lies a quarter of the way from its own block's sample towards the block above or below. At the edges of the picture the nearest sample stands in for the missing one.
- **The colour filter** of the video is applied to the RGBA as it is made. Each step has the meaning CSS gives it and is clamped after it; the result may differ from the exact fraction by one level of 255. The card applies filters of up to eight steps; a video with a longer one is made RGBA in memory.
- **A video drawn at a size with an odd side** cannot be 4:2:0. Its reader hands it over as RGBA (`frx7/docs/spec/pixel.md`, section 7), and only the colour filter is applied, in memory.
- **Decoding** is the system's, on the machine's hardware where it has it. The pictures are those of the standard: for H.264 and HEVC they were compared with another decoder's and are the same to the byte. Size, colour and RGBA are our own arithmetic, so the picture does not depend on which decoder ran.
- **Which picture a frame shows.** The latest picture of the source whose time is at or before the frame's time; never a later one. For a source at the composition's frame rate that is the picture of the same number. A source at another rate, or one whose rate changes, repeats or passes over pictures by this rule. Source: experiment E7 of `POC_PLAN.md`, where sources at 25 and 23.976 pictures a second showed a picture too early in a quarter of the frames.
- **Starting inside a source.** A reader may start at any frame. It goes to the last picture of the source that can be decoded by itself at or before the frame's time and decodes forward from there, so that the picture at or before the frame's time is the one shown.
- **Interlaced sources** are deinterlaced: each frame is one picture, made from the upper field alone (`frx7/docs/spec/pixel.md`, section 4), with no rows of another picture between its rows.
- **A rotation mark** on the source is followed: the picture is turned as marked, and the element's `width` and `height` are those of the turned picture.
- **More than 8 bits.** A source with more bits in ordinary (standard range of brightness) video is brought to 8 bits.
- **Refused before the render starts**, with the element's line and the file's name: a video marked as HDR (its transfer is PQ or HLG), because its brightness would be shown wrongly; and a video with transparency (HEVC pictures in more than one layer), because its see-through parts would be shown black. Both are left for later.
- **A stream the system does not decode.** Whether the system decodes a stream is known for certain only when it is given a picture: some streams of a codec it has are not decoded (H.264 kept without loss was refused when its frames hold two fields, and at a very small size). So the first picture of every video is decoded when the video is looked at, and a video whose first picture cannot be decoded is refused before the render starts, with the element's line, the file's name and the system's reason.
- **A decoder reads forward only.** A video asked for a picture well ahead of the one its decoder is at, or behind it, gets a new decoder that starts at that picture.

## 5. Several workers

One worker is the three stages of section 1. A render may run several workers at the same time, each on its own stretch of the film.

- **The stretches.** The frames to render are shared out evenly, in order, one stretch for each worker. A stretch is never shorter than one second, so a short render gets fewer workers than were asked for.
- **Each worker is whole.** It has a renderer, decoders and an encoder of its own and writes its stretch to a file of its own. Nothing is shared between workers but the composition and the decoded still pictures, which no worker changes.
- **The pictures do not depend on the number of workers.** A frame is drawn from the composition and its number alone, and a video starts at the picture its frame names wherever a stretch begins. Only the encoder's choices differ: each stretch begins with a picture that is encoded whole.
- **Joining.** The parts are put one after the other into one file without encoding again (`frx7/docs/spec/mp4.md`, section 2), and the file of sound, when there is one, becomes its sound track in the same pass. The part files and the file of sound are then removed. A film of one worker with sound goes the same way.
- **A failure in one worker stops the others** at their next frame, and the render ends with the message of the worker that failed.
- **The sound** is mixed once for the whole film, beside the workers.
- **`auto`** is one worker for the GPU renderer, where a second worker was measured to add next to nothing because one already keeps the card busy; and a third of the machine's cores, at most four, for the CPU renderer, which draws each worker's frames on one core. Measurements on more machines may change this rule.

## 6. A frame on request

`render` draws frames in order. A preview asks for them in any order. `poc seek <project folder | framesmith.yaml | composition.xml> --list <file> [--backend auto|cpu|gpu]` holds a composition open and draws the frames the list names, one number on a line, in the order of the list.

- **The picture of a frame is the one `render` draws for it**, whatever was asked for before. Both draw a frame from the composition and its number alone.
- **Footage.** A decoder reads forward only (section 4). The last 64 pictures a video's decoder gave are kept, and a picture among them is given from there. One up to 90 pictures ahead of the decoder is read on to, and the pictures passed on the way are kept too. Any other picture needs a new decoder: after a step back (to a picture at most 64 behind the decoder) it starts 45 pictures before the one wanted, so that the next steps back find their pictures kept; after a jump it starts at the picture wanted. No decoder is started ahead of its clip, since the next frame asked for is not known.
- **Decoders kept.** The decoder of a video, and its kept pictures, stay for the next eight requests after the last one that showed it, so that a step back across a cut does not start it again.
- **What it prints.** For each frame a line with its number, the time from the request to the pixels, how many decoders were started for it, and a mark of its pixels (16 hexadecimal digits; equal pixels give equal marks). Then the median, the time nine in ten stayed under, and the slowest.
- **`render --no-encode --marks <file>`** writes the same mark for every frame it draws, one line for each frame: the number, then the mark. This is how a frame on request is compared with the frame of a render in order.
- **`--dump <folder>`**, on `seek` and on `render --no-encode`, writes the RGBA pixels of each frame to a file of its own in the folder: `<frame>.rgba` for a render, `<request>-<frame>.rgba` for a frame on request, where the request is its place in the list from 1. It is for looking at a difference the marks have shown.

## 7. The sound, in stretches

The rules of the mix are in `native-notation.md`, section 4. This section says how it is made, so that a long film needs no more memory than a short one. Source: experiment E5 of `POC_PLAN.md`, where the mix of an hour held 2.5 GB.

- **Stretches.** The mix is made a few seconds at a time, from the first second of the film to the last. Neither a source nor the mix is ever held whole in memory.
- **Sources.** Each sound has a decoder of its own, started for the stretch its sound begins in and ended with its sound. Only the sounds that play in a stretch have a decoder then. A source is whatever sound file the system reads (WAV, AAC in MP4, MP3 and others), with one, two or six channels; it is brought to 48,000 samples a second and two channels by frx7's own arithmetic (`frx7/docs/spec/audio.md`).
- **The same samples.** A sample of the mix is the sum of its sounds in the order they are written, as before; making it in stretches changes no sample.
- **The peak.** The highest peak is followed from stretch to stretch, between the samples as well as on them and across the joins of two stretches. It is known only when the last stretch is made.
- **Two passes.** The mix as made is therefore written to a file beside the output while it is made. When the peak is known, that file is read again, lowered if the peak asks for it, and given to the system's AAC encoder, which writes a file of sound alone. The file of the mix is then removed. The file of sound becomes the sound track of the finished film when the film is put together (section 5), in one pass, without encoding again; its length is the film's to the sample. It takes 384 kB for each second of film.
- **A lead-in.** An AAC or MP3 source starts with a short lead-in that is not part of its sound. The decoder passes over it wherever the sound is played from, so a sound is on time to the sample (experiment E7).
- **A source of one channel** plays on both sides, each 3 dB lower than the source.
- **A source that cannot be decoded** stops the render with the name of the file and the reason. A source shorter than its clip is silent from where it ends.

## 8. Turned and opening groups on the card

Sources: experiment E6 of `POC_PLAN.md`; `native-notation.md`, section 8, for what a turn and an opening are.

- The CPU renderer draws the group flat into a picture in memory, and turns or opens that picture there. It defines the result.
- The GPU renderer does the same three things on the card. The drawing library draws the group flat into a texture of its own, as large as the part of the frame the group can draw on (for a turn, up to a frame's width and height around the frame). A pass of ours reads that texture and writes the turned or opened picture into a second texture as large as the frame, only inside the part the result covers. That texture is put on the frame like a picture, with the group's blend.
- The pass uses the arithmetic of the CPU renderer: a pixel of the result is taken from between the four pixels of the flat picture around its place, with colours multiplied by their alpha and nothing outside the picture; a turn finds the place with the inverse of the turn's matrix; an opening multiplies by the share shown.
- An opening is a function of the form `native-notation.md` gives; the two that are built in are written in that form too, so an opening written by the author goes the same way.
- A group that is itself blurred or brightened, or whose flat picture would be larger than the card's largest texture, is drawn by the painter in memory as before. A shader opening cannot go that way and is refused with the element's reason.
- The opening functions are compiled when the renderer starts, so a function the compiler does not take stops the render before the first frame.

### What `check` says of an opening written as a shader

`check <project folder | project file | composition>` reads the composition as a render does and draws nothing.

- For a valid composition it prints one line that says so, with the number of frames, the size and the frame rate, and ends with success.
- For each opening written as a shader it prints one more line: the line of the first element that names it, the file, and that the composition can be rendered only with the GPU renderer and is not checked against the CPU renderer.
- When the composition has such openings, `check` also starts the GPU renderer, so that a function the card's compiler does not take is reported here, with its file and the compiler's words, and `check` fails. On a machine with no graphics card `check` fails and says that the composition cannot be rendered on this machine.
- `render` with the renderer left to choose itself does not fall back to the CPU renderer for such a composition: it fails with the card's reason and the name of the opening that needs the card.
