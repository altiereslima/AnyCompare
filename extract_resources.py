import json
import os
import sys

def main():
    json_path = os.path.join(os.environ["TEMP"], "bc4_resources.json")
    exe_path = r"C:\Projetos\BC4\bc4\BCompare.exe"
    png_dir = r"C:\Projetos\BC4\anycompare\assets\png"
    forms_dir = r"C:\Projetos\BC4\anycompare\assets\forms"

    with open(json_path, "r", encoding="utf-8-sig") as f:
        data = json.load(f)

    resources = data["normalized_result"]["resources"]

    with open(exe_path, "rb") as exe:
        png_count = 0
        form_count = 0
        
        for r in resources:
            r_type = r.get("type", {})
            r_name = r.get("name", {})
            payload = r.get("payload", {})
            loc = payload.get("location", {})
            offset = loc.get("offset")
            length = loc.get("bytes")

            if offset is None or length is None:
                continue

            name_str = r_name.get("name") or f"id_{r_name.get('id')}"
            type_name = r_type.get("name")
            type_id = r_type.get("id")

            exe.seek(offset)
            bytes_data = exe.read(length)

            # PNG extraction
            if type_name == "PNG" or bytes_data.startswith(b"\x89PNG\r\n\x1a\n"):
                out_path = os.path.join(png_dir, f"{name_str}.png")
                with open(out_path, "wb") as out:
                    out.write(bytes_data)
                png_count += 1

            # Delphi Form (TPF0) extraction
            elif type_id == 10 and (name_str.startswith("T") or bytes_data.startswith(b"TPF0")):
                out_path = os.path.join(forms_dir, f"{name_str}.tpf0")
                with open(out_path, "wb") as out:
                    out.write(bytes_data)
                form_count += 1

    print(f"Extracted {png_count} PNGs to {png_dir}")
    print(f"Extracted {form_count} Delphi Forms to {forms_dir}")

if __name__ == "__main__":
    main()
