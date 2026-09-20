from PIL import Image

def analyze(path):
    im = Image.open(path).convert('RGB')
    w, h = im.size
    px = im.load()
    # Heuristic: background (outside square) is either near-white or near the darkest square.
    # Find dark-square pixels: brightness < 60 OR (we'll find bbox of 'not near-white').
    minx, miny, maxx, maxy = w, h, 0, 0
    found = 0
    for y in range(h):
        for x in range(w):
            r, g, b = px[x, y]
            # square body is dark navy ~ (28,30,38); margin is white ~ (240+)
            if r < 120 and g < 120 and b < 130:
                found += 1
                if x < minx: minx = x
                if x > maxx: maxx = x
                if y < miny: miny = y
                if y > maxy: maxy = y
    print(f'{path} {w}x{h} darkpx={found} bbox_of_dark=({minx},{miny})-({maxx},{maxy})')

for p in ['elsewhen-concept-icon.png', 'elsewhen-icon-v2-1024.png']:
    try:
        analyze(p)
    except FileNotFoundError:
        print(p, 'NOT FOUND')
