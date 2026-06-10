# wyrd-cards

Local card holders and filesystem materialization helpers.

```python
from wyrd import Workflow

run = Workflow.load("research.yaml").run("climate change")
print(run.outcomes)
```

Card `save` and `load` are local filesystem operations. Durable registration is
owned by registry/client surfaces.
