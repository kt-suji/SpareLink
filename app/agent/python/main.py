import time
import json
import platform
import socket
import psutil

# PCの静的スペック情報（起動時に1回取得）
cpu_cores = psutil.cpu_count(logical=True)
total_memory = round(psutil.virtual_memory().total / (1024 ** 3), 1)
pc_name = socket.gethostname()
os_name = f"{platform.system()} {platform.release()}"

while True:
    mem = psutil.virtual_memory()
    data = {
        "pc_name": pc_name,
        "os": os_name,
        "cpu_cores": cpu_cores,
        "cpu_percent": round(psutil.cpu_percent(), 1),
        "memory_percent": round(mem.percent, 1),
        "memory_total_gb": total_memory,
        "memory_available_gb": round(mem.available / (1024 ** 3), 1),
    }

    print(json.dumps(data), flush=True)

    time.sleep(1)