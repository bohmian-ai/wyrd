# What a model sends as code.run source: list the notes, read each one, and
# search the note over the limit. The notes stay in Python; the model sees
# only the printed hits and match count.
import json

listing = await call_tool('fs.list', {'path': '/inputs'})
paths = sorted(entry['path'] for entry in listing['files'])
hits = []
for path in paths:
    read = await call_tool('fs.read', {'path': path})
    if 'OVER-LIMIT' in read['units'][0]['text']:
        hits.append(path)
found = await call_tool('fs.search', {'query': 'OVER-LIMIT', 'path': hits[0]})
print(json.dumps({'hits': [path.split('/')[-1].split('.')[0] for path in hits], 'matches': len(found['matches'])}))
