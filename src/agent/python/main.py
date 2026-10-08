import time
import json
import psutil

while True:
    data = {
        "cpu": psutil.cpu_percent(),
        "memory": psutil.virtual_memory().percent,
    }

    print(json.dumps(data), flush=True)

    time.sleep(1)