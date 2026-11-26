# Christmas sprites

Created with the built-in `image_gen.imagegen` tool, each asset generated separately
with `transparent_background: true`.
The selected PNG files were copied into this directory without image processing.
The existing runtime PNG decoder premultiplies alpha before GPU upload.

## tree.png

Original: `exec-b5523ced-ff5e-42f8-822a-08f2259a375f.png`.

```text
Use case: illustration-story. Asset type: single transparent desktop overlay game sprite, Christmas tree. Create one beautiful full height decorated evergreen Christmas tree, front view, star at top, short trunk visible, no floor, no background. Cozy vintage hand painted 1940s cel animation aesthetic, expressive organic silhouette, richly shaded dark forest green boughs, warm golden star, red and gold ornaments, gentle snow resting on some branches. The tree has a thin winding dark cable but NO glowing lightbulbs: animated colored bulbs will be added in code. Compact triangular tree silhouette. Render the complete tree with generous transparent margin, portrait composition, isolated genuine transparent alpha background. Fine clean edges, warm traditional animated film craft, legible at 250px height. No text, no watermark, no frame, no ground shadow, no scene, no other objects. Final PNG intended path windows/assets/christmas/tree.png.
```

## tractor.png

Original: `exec-3eec4a04-0ff3-4110-978c-9d0dbc46294c.png`.

```text
Use case: illustration-story. Asset type: transparent 2D side view Christmas desktop game sprite. One charming vintage red tractor snowplow facing RIGHT, full vehicle in true side profile, entirely visible. Cozy hand painted classic 1940s cel animation art, dark ink contours, warm rounded shapes, festive holly small trim, amber roof beacon, cream window reflecting winter blue, red steel body, snow on roof, two clearly separated large dark rubber wheels resting on same baseline, front snowplow blade extends to RIGHT. No people. Clean silhouette, readable when only 90 pixels high, soft shaded painted color, matching a vintage hand drawn Christmas animation. Vehicle occupies almost entire wide image with small clear margin. Genuine transparent background outside vehicle, no ground, no snowdrift, no flying snow, no cast shadow, no text or watermark. Wheels and body in same asset are fine: wheel rotation and snow spray will be added by shader. Final PNG intended path windows/assets/christmas/tractor.png.
```

## santa.png

Original: `exec-b801106b-627e-4967-91a9-6274d1c9e7df.png`.

```text
Use case: illustration-story. Asset type: single wide transparent 2D side view Christmas game sprite. Santa Claus in a beautiful red gold sleigh pulled along the ground by TWO REINDEER facing RIGHT, all in full true SIDE PROFILE, complete group entirely visible. Sleigh at LEFT, Santa seated warmly smiling with white beard and red coat, small sack of gifts, visible gold curved runners at ground; two brown reindeer with branched antlers and red harness at RIGHT, one following the other, connected harness reins clearly visible, lively mid-trot poses with separated legs, hooves at same ground baseline as sleigh runners. Cozy rich hand painted vintage 1940s cel animation aesthetic with warm ink lines and subtle shaded gouache color, charming expressive animals. High quality refined natural animal anatomy, easily legible at small size. Ground travel, absolutely NOT flying, no landscape and no magic trail. Isolated genuine transparent alpha background, no floor, no ground shadow, no clouds, no snowdrift, no text, no watermark. Compact wide composition, all antlers and runners inside canvas, minimal transparent margin. Animation will later deform lower leg parts and rock the sleigh gently in shader. Final PNG intended path windows/assets/christmas/santa.png.
```

## Animation

Colored lamp cores, halos, continuous sagging wires and the placement border are shader
geometry rather than baked into the sprites. Tractor wheel interiors rotate in texture
coordinates; its body bounces slightly, the beacon pulses and a separate snow-dust pass
appears at the plow. The sleigh rocks gently. Reindeer now use eight articulated
legs with separate thigh, shank and hoof layers. The current gait interpolates
measured poses from a gallop reference, including takeoff, flight and landing. The complete
tree and its lights are drawn after all transport and snow spray, as foreground scenery.
All animation is independent of snowfall.


## Articulated reindeer update

Each part below was created with built-in `image_gen.imagegen`,
`transparent_background: true`, and `referenced_image_paths` set to the named local
reference, after viewing that reference. All selected outputs were copied intact
from the generated-images directory; no pixels were erased or painted by code.

### santa-body.png

Reference: `santa.png`. Original: `exec-cbb86bd9-9dc0-46f4-99ed-0253d5e59bb4.png`.

```text
Use case: precise-object-edit. Edit target: the attached existing transparent Santa sleigh with two reindeer game sprite. Create a production replacement BODY LAYER for a skeletal 2D animation. Keep Santa, sleigh, gifts, golden runners, heads, antlers, faces, ears, necks, tails, torso silhouettes, harness and reins visually IDENTICAL to the reference, in the SAME positions and proportions and the SAME wide 1983x793 canvas framing. Preserve the warm vintage painted animation style and fine ink details. CHANGE ONLY THIS: remove ALL eight legs and hooves of the two reindeer, replacing those leg regions with genuine transparent alpha; restore a clean rounded natural torso underside and small rounded shoulder/hip attachment surfaces at the leg roots. Reindeer should now be complete HEAD-AND-TORSO cutouts floating above where legs used to be; NO long downward leg stubs and NO feet remain. Do NOT remove sleigh runners, rein straps, necks or chests. Keep the original full group width and original large transparent empty region below deer torsos to align separate animated legs later. Preserve current genuine transparency outside the painted subjects. Absolutely no background, no shadow, no extra objects, no text, no watermarks. This is surgical animation layer extraction, not a redesigned illustration.
```

### deer-thigh.png

Reference: `santa.png`. Original: `exec-dc266fb5-72e9-4fa3-9bae-9c1f6175d64d.png`.

```text
Use case: illustration-story. Reference image: use the attached Santa/reindeer illustration ONLY as exact art-style and fur color reference. Produce ONE isolated animation part: the UPPER LEG / THIGH of one of those warm brown painted reindeer, a short softly tapered fur-covered limb segment, no torso, no head, no lower leg, NO hoof. Straight neutral limb pointing VERTICALLY DOWN, upper shoulder/hip joint centered near the top and knee joint centered near the bottom. Both ends rounded and fully painted, no mechanical joint holes, no ball sockets, no visible bones; fluffy overlap at the wider upper joint and a narrower gently rounded knee. Warm golden chestnut brown fur with the same vintage gouache texture, dark warm ink contours and gentle cream highlight on front edge as the reference deer. Shape is slender, about 2.6 times taller than its width, not a cylindrical human arm. Complete part centered on a genuine transparent alpha canvas with small even margins, no background, no floor or shadow, no text or other parts, no diagram or labels. This will be scaled and rotated as a painted puppet's upper leg. Match the reference texture/colors precisely.
```

### deer-shin.png (intermediate, not loaded at runtime)

Reference: `santa.png`. Original: `exec-3b7af1c1-55e9-423e-8d23-ee2ea86fe6c7.png`.

```text
Use case: illustration-story. Reference image: attached Santa/reindeer illustration is the exact painted art-style and brown fur reference. Produce ONE isolated animation part: a reindeer's LOWER LEG / SHIN WITH DARK CLOVEN HOOF at its lower end, in matching vintage painted cel animation style. Straight neutral limb pointing VERTICALLY DOWN. The gently rounded knee joint is centered near the top; the ankle and dark brown hoof occupy the bottom. Slender warmly chestnut golden brown fur, warm dark ink contour, gentle cream highlight along the front RIGHT edge, softly furry rounded top that can overlap an upper leg. The little dark hoof has two subtle toes, pointing slightly RIGHT, a flat bottom to rest on snow, anatomically recognizable as a reindeer hoof. No upper leg or thigh, no torso, no head, no antler, NO extra parts. Part is about 3 times taller than its width. Fully visible centered part on genuinely transparent alpha background, minimal even margin; no floor, no cast shadow, no labels, no text, no mechanical joint holes or sockets. This is one complete detachable puppet part to rotate as a limb, preserve a clean painted silhouette and exact brown palette of the reference.
```

### deer-shank.png

Reference: `deer-shin.png`. Original: `exec-f1a85051-5ffd-433b-99a6-873660c58508.png`.

```text
Use case: precise-object-edit. Edit target: attached isolated painted reindeer lower leg with hoof. Make the exact same animation limb layer BUT remove ONLY the dark brown cloven hoof at its bottom, leaving the warm brown/cream furry shin and ankle intact, ending in a small clean rounded furry ankle joint where the dark hoof originally joined. Keep original pixels/style/straight vertical silhouette/width/proportions and transparent canvas framing as close as possible. Retain the knee tuft at top and painted fur. The dark hoof will be a separate animated piece: absolutely NO hoof, no dark toes, no foot remain attached to the shin in this output. Genuine transparent alpha everywhere outside the fur-covered lower leg; do not fill removed hoof area with a background. No new objects, no shadow, no text, no diagram. This is a surgical separation of an existing painted puppet part.
```

### deer-hoof.png

Reference: `deer-shin.png`. Original: `exec-725c9098-f817-4d50-95d8-edfe7abb721a.png`.

```text
Use case: precise-object-edit. Reference/edit target: attached reindeer lower leg with a dark brown hoof. Extract and complete ONLY its single DARK BROWN CLOVEN HOOF as an isolated animation part, no leg or shin. Preserve the same vintage warm painted cel/gouache texture, ink contour, two subtle toes and chocolate brown colors as this reference hoof. Side profile with toe pointing RIGHT. Broad flat horizontal sole along bottom so it can stand on snow. Soft rounded slightly open ankle attachment at top for overlapping a furry leg; fully paint the hidden upper heel surface to complete the standalone part. Compact shape about 1.4 times wider than tall. The hoof fills most of a small square genuinely transparent canvas with even thin margins. Exactly one hoof only, no torso, no limb, no knee, no background, no ground shadow, no text/labels/watermark. This part will rotate independently, so keep the sole level.
```
