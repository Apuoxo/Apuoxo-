# Queue

Place one UTF-8 JSON task per file here. The worker processes `*.json` and writes `results/<id>.json`.

Tasks are data, not shell code. Only the allowlisted tools in `protocol/task.schema.json` are accepted.

Do not put secrets in queue files.
