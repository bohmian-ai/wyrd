# What a model sends as code.run source: read a Card, then query Bifrost.
# A denied call raises RuntimeError carrying the catalog problem JSON, so the
# program can branch on its code and carry on without the protected value.
import json

card = await call_tool('cards.get', {'card_ref': {'kind': 'Service', 'name': 'policy-desk', 'version': '1.0.0'}})
try:
    await call_tool('bifrost.query', {'sql': 'SELECT execution_id FROM bifrost.verification.results LIMIT 1', 'max_rows': 1})
    query = 'allowed'
    again = None
except RuntimeError as error:
    query = json.loads(str(error))['code']
    again = await call_tool('cards.get', {'card_ref': {'kind': 'Service', 'name': 'policy-desk', 'version': '1.0.0'}})
# The model sees what the program prints and the repr of its last expression,
# here None; every other value stays in Python.
print('query', query, 'repaired', again == card)
