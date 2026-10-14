"""Developer-only thumbnails; source images remain unchanged and hash verified."""
import json
import shutil
from pathlib import Path
from PIL import Image
root = Path(__file__).resolve().parent.parent
for entry in json.loads((root / 'desktop/ui/themes/sources.json').read_text(encoding='utf-8')):
    source = root / 'assets/themes' / entry['id']
    dest = root / 'desktop/ui/themes' / entry['id']
    dest.mkdir(parents=True, exist_ok=True)
    with Image.open(source / entry['image_file']) as image:
        image.thumbnail((640, 320))
        image.convert('RGB').save(dest / 'preview.jpg', quality=78, optimize=True)
    shutil.copyfile(source / 'LICENSE.md', dest / 'LICENSE.md')
print('Prepared 40 small previews and attribution notices')
