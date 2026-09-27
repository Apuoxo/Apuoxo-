# State contract

The repository is persistent state, not the running process.

Every external task has a stable task id, explicit tool name, input arguments, a result file, success or failure, and timestamps.

Results must be sufficient for Virt to resume reasoning without trusting unstated runner-local state.

The queue, results, and state directories form a checkpoint protocol rather than conversational memory.
