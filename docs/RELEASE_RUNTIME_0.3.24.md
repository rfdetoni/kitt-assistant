# Assistant Runtime 0.3.24 — Agent CLI 0.87.12 lock alignment

Update all relevant `uv.lock` dependency revisions to the verified Agent CLI commit `4b955a82633fccc4ba6ffef3c97703b1dfda0916`, maintaining Protocol 0.13.3 and the existing native/toolbox APIs. The changed Agent CLI enforces an initial KAP/2 host tool request for durable Goals steps rather than accepting unverified WebChat final reports. Consumers, docs and CI pins are synchronized; no new dependencies added.
