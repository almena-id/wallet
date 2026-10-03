"""Adds the notification's words (en.lproj, es.lproj Localizable.strings) and
the privacy manifest (PrivacyInfo.xcprivacy) to the generated Xcode project, as
`push/sync.sh` asks.

The project is edited rather than generated again: the xcodegen on a machine is
not necessarily the one `tauri ios init` used, and regenerating renames the
product and rewrites the scheme. Safe to run again: it does nothing to a
project that already has them.
"""

import sys

path = sys.argv[1]
text = open(path).read()


def insert_before(marker, lines):
    global text
    if marker not in text:
        sys.exit(f"{path}: no {marker!r}")
    text = text.replace(marker, lines + marker, 1)


if "/* Localizable.strings */" not in text:
    # Fixed ids, so running this on a fresh project gives the same file.
    BUILD = "A1A1E0000000000000000001"
    GROUP = "A1A1E0000000000000000002"
    LANGS = {"en": "A1A1E0000000000000000003", "es": "A1A1E0000000000000000004"}

    insert_before(
        "/* End PBXBuildFile section */",
        f"\t\t{BUILD} /* Localizable.strings in Resources */ = "
        f"{{isa = PBXBuildFile; fileRef = {GROUP} /* Localizable.strings */; }};\n",
    )
    insert_before(
        "/* End PBXFileReference section */",
        "".join(
            f"\t\t{ref} /* {lang} */ = {{isa = PBXFileReference; lastKnownFileType = text.plist.strings; "
            f'name = {lang}; path = {lang}.lproj/Localizable.strings; sourceTree = "<group>"; }};\n'
            for lang, ref in LANGS.items()
        ),
    )
    variant = (
        f"\t\t{GROUP} /* Localizable.strings */ = {{\n"
        "\t\t\tisa = PBXVariantGroup;\n\t\t\tchildren = (\n"
        + "".join(f"\t\t\t\t{ref} /* {lang} */,\n" for lang, ref in LANGS.items())
        + '\t\t\t);\n\t\t\tname = Localizable.strings;\n\t\t\tsourceTree = "<group>";\n\t\t};\n'
    )
    if "/* End PBXVariantGroup section */" in text:
        insert_before("/* End PBXVariantGroup section */", variant)
    else:
        text = text.replace(
            "/* End PBXSourcesBuildPhase section */\n",
            "/* End PBXSourcesBuildPhase section */\n\n/* Begin PBXVariantGroup section */\n"
            + variant
            + "/* End PBXVariantGroup section */\n",
            1,
        )

    # In the wallet_iOS group, beside its Info.plist.
    entitlements = "/* wallet_iOS.entitlements */,\n"
    at = text.index(entitlements, text.index("/* wallet_iOS */ = {"))
    text = (
        text[: at + len(entitlements)]
        + f"\t\t\t\t{GROUP} /* Localizable.strings */,\n"
        + text[at + len(entitlements) :]
    )

    # Copied into the bundle.
    phase = text.index("isa = PBXResourcesBuildPhase;")
    files = text.index("files = (\n", phase) + len("files = (\n")
    text = text[:files] + f"\t\t\t\t{BUILD} /* Localizable.strings in Resources */,\n" + text[files:]

    # Spanish is a region the project knows.
    regions = text.index("knownRegions = (")
    end = text.index(");", regions)
    if "\t\t\t\tes,\n" not in text[regions:end]:
        text = text[:end] + "\tes,\n\t\t\t" + text[end:]


# The privacy manifest, beside Info.plist in the wallet_iOS group, copied into
# the bundle.
if "/* PrivacyInfo.xcprivacy */" not in text:
    BUILD = "A1A1E0000000000000000005"
    FILE = "A1A1E0000000000000000006"
    insert_before(
        "/* End PBXBuildFile section */",
        f"\t\t{BUILD} /* PrivacyInfo.xcprivacy in Resources */ = "
        f"{{isa = PBXBuildFile; fileRef = {FILE} /* PrivacyInfo.xcprivacy */; }};\n",
    )
    insert_before(
        "/* End PBXFileReference section */",
        f"\t\t{FILE} /* PrivacyInfo.xcprivacy */ = {{isa = PBXFileReference; "
        f'lastKnownFileType = text.xml; path = PrivacyInfo.xcprivacy; sourceTree = "<group>"; }};\n',
    )
    entitlements = "/* wallet_iOS.entitlements */,\n"
    at = text.index(entitlements, text.index("/* wallet_iOS */ = {"))
    text = (
        text[: at + len(entitlements)]
        + f"\t\t\t\t{FILE} /* PrivacyInfo.xcprivacy */,\n"
        + text[at + len(entitlements) :]
    )
    phase = text.index("isa = PBXResourcesBuildPhase;")
    files = text.index("files = (\n", phase) + len("files = (\n")
    text = text[:files] + f"\t\t\t\t{BUILD} /* PrivacyInfo.xcprivacy in Resources */,\n" + text[files:]

open(path, "w").write(text)
