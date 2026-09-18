#!/usr/bin/env python3
import argparse
import hashlib
import json
from pathlib import Path, PurePosixPath
import zipfile


def main():
    parser = argparse.ArgumentParser(description="Index a GFN archive and recover source-map text for local auditing.")
    parser.add_argument("archive", type=Path)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    records = []
    with zipfile.ZipFile(args.archive) as archive:
        for entry in archive.infolist():
            path = PurePosixPath(entry.filename)
            if path.is_absolute() or ".." in path.parts:
                raise ValueError(f"Unsafe archive path: {entry.filename}")
            if entry.is_dir():
                continue
            data = archive.read(entry)
            destination = args.output / "archive" / path
            destination.parent.mkdir(parents=True, exist_ok=True)
            destination.write_bytes(data)
            record = {"path": entry.filename, "bytes": len(data), "sha256": hashlib.sha256(data).hexdigest()}
            if path.suffix == ".map":
                source_map = json.loads(data)
                sources = source_map.get("sources", [])
                contents = source_map.get("sourcesContent", [])
                recovered = []
                map_id = hashlib.sha256(entry.filename.encode()).hexdigest()[:16]
                for index, (source, content) in enumerate(zip(sources, contents)):
                    if content is None:
                        continue
                    name = PurePosixPath(source).name or "source.txt"
                    output = Path("sources") / map_id / f"{index:05d}-{name}"
                    target = args.output / output
                    target.parent.mkdir(parents=True, exist_ok=True)
                    target.write_text(content, encoding="utf-8")
                    recovered.append({"source": source, "output": str(output), "lines": len(content.splitlines())})
                record["sources"] = recovered
            records.append(record)
    with args.archive.open("rb") as source:
        digest = hashlib.file_digest(source, "sha256").hexdigest()
    manifest = {"archive": args.archive.name, "sha256": digest, "files": records}
    (args.output / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({"archive_sha256": digest, "files": len(records), "recovered_sources": sum(len(r.get("sources", [])) for r in records)}))


if __name__ == "__main__":
    main()
