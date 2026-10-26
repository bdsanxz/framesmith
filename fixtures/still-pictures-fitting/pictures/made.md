# How these pictures were made

All four are our own material, made once by hand for this fixture; nothing here comes from a third party.

- `marked.png` (8 by 4, RGBA, every alpha 255): grey 128, with its four corner pixels red (255, 0, 0) at the top left, green (0, 255, 0) at the top right, blue (0, 0, 255) at the bottom left and yellow (255, 255, 0) at the bottom right.
- `glass.png` (4 by 4, RGBA): every pixel (255, 0, 0) at alpha 128.
- `quarters.png` (16 by 16, RGBA, every alpha 255): four quarters of 8 by 8, (200, 40, 40) at the top left, (40, 200, 40) at the top right, (40, 40, 200) at the bottom left and (220, 220, 60) at the bottom right.

The three PNG files were written by a short script with Python's `zlib` (filter 0 on every row, one `IDAT`). `crates/render/tests/still_pictures_fixtures_worked_by_hand.rs` reads them back and holds their pixels to the values above.

`quarters.jpg` was made from `quarters.png` by the system's own tool:

    /usr/bin/sips -s format jpeg -s formatOptions best pictures/quarters.png --out pictures/quarters.jpg
