"""The caller's desktop data search path crosses sudo only as validated data."""

from pathlib import Path


def validated_data_dirs(value):
    if value is None or value == "":
        return "/usr/local/share:/usr/share"
    if (
        not isinstance(value, str)
        or not value.isprintable()
        or any(not Path(directory).is_absolute() for directory in value.split(":"))
    ):
        raise ValueError(
            "XDG_DATA_DIRS must contain nonempty absolute paths without control characters"
        )
    return value
