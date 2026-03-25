#!/usr/bin/env python3

import json
import subprocess
from pathlib import Path

MANIFEST_PATH = Path("manifest.json")
PREVIEW_ROOT = Path("assets/previews/eoj/main_sets")
THUMBNAIL_WIDTH = 320
JPEG_QUALITY = 75


def main() -> None:
    manifest = json.loads(MANIFEST_PATH.read_text())
    asset_root = Path(manifest["asset_root"])
    preview_root = PREVIEW_ROOT

    generated = 0
    missing_base = []
    ordered_cards = {}

    for card_id, card in manifest["cards_by_id"].items():
        base_asset = next(
            (asset for asset in card["assets"] if asset["variant"] == "base"),
            None,
        )

        if base_asset is None:
            preview_relative_path = None
            missing_base.append(card["card_id"])
        else:
            relative_path = Path(base_asset["relative_path"])
            source_path = asset_root / relative_path
            preview_path = preview_root / relative_path
            preview_path.parent.mkdir(parents=True, exist_ok=True)

            subprocess.run(
                [
                    "magick",
                    str(source_path),
                    "-strip",
                    "-thumbnail",
                    f"{THUMBNAIL_WIDTH}x",
                    "-quality",
                    str(JPEG_QUALITY),
                    str(preview_path),
                ],
                check=True,
            )

            preview_relative_path = relative_path.as_posix()
            generated += 1

        ordered_cards[card_id] = {
            "set_name": card["set_name"],
            "card_id": card["card_id"],
            "title_slug": card["title_slug"],
            "preview_relative_path": preview_relative_path,
            "review_flags": card["review_flags"],
            "assets": card["assets"],
        }

    ordered_manifest = {
        "asset_root": manifest["asset_root"],
        "preview_root": preview_root.as_posix(),
        "cards_by_id": ordered_cards,
    }

    MANIFEST_PATH.write_text(
        json.dumps(ordered_manifest, indent=2, ensure_ascii=False) + "\n"
    )

    print(f"generated {generated} previews into {preview_root}")
    if missing_base:
        print(f"cards without base asset: {', '.join(missing_base)}")


if __name__ == "__main__":
    main()
