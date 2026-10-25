"""Compile actual RocksDB headers in two units and verify shared table identity."""

import os
from pathlib import Path
import shlex
import subprocess
import sys
import tempfile


TABLES = (
    "global_operation_table",
    "global_op_stage_table",
    "global_state_table",
    "compaction_operation_properties",
    "flush_operation_properties",
)


def main():
    crate = Path(__file__).resolve().parents[1]
    rocksdb = crate / "rocksdb"
    with tempfile.TemporaryDirectory(prefix="rocksdb-table-identity-") as directory:
        temporary = Path(directory)
        addresses = ", ".join(f"ROCKSDB_NAMESPACE::{name}" for name in TABLES)
        first = temporary / "first.cc"
        first.write_text(
            '#include "util/thread_operation.h"\n'
            "#include <cstdio>\n"
            'extern "C" void second(const void**);\n'
            "int main() {\n"
            f"  const void* first[] = {{{addresses}}};\n"
            "  const void* other[5];\n"
            "  second(other);\n"
            "  for (int i = 0; i < 5; ++i) {\n"
            "    if (first[i] != other[i]) {\n"
            '      std::fprintf(stderr, "Duplicated thread status table %d\\n", i);\n'
            "      return 1;\n"
            "    }\n"
            "  }\n"
            "  return 0;\n"
            "}\n"
        )
        second = temporary / "second.cc"
        assignments = "\n".join(
            f"  output[{index}] = ROCKSDB_NAMESPACE::{name};"
            for index, name in enumerate(TABLES)
        )
        second.write_text(
            '#include "util/thread_operation.h"\n'
            'extern "C" void second(const void** output) {\n'
            + assignments
            + "\n}\n"
        )
        executable = temporary / "identity"
        compiler = shlex.split(os.environ.get("CXX", "c++"))
        linker = "-Wl,-dead_strip" if sys.platform == "darwin" else "-Wl,--gc-sections"
        subprocess.run(
            compiler
            + [
                "-std=c++20",
                "-DNDEBUG",
                "-ffunction-sections",
                "-fdata-sections",
                f"-I{rocksdb / 'include'}",
                f"-I{rocksdb}",
                str(first),
                str(second),
                str(rocksdb / "monitoring/thread_status_impl.cc"),
                linker,
                "-o",
                str(executable),
            ],
            check=True,
        )
        subprocess.run([str(executable)], check=True)
    print("All five thread status tables share one definition across translation units")


if __name__ == "__main__":
    main()
