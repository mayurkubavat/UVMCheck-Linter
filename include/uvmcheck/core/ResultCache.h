#pragma once

// Incremental result cache (.uvmcheck_cache/). A compilation unit is one
// primary source file. Its key hashes the tool version, the rule set and
// options, waiver and config file bytes, the preset and defines and include
// dirs, and the bytes of that file, every file it transitively includes, and
// every package it imports (from slang's source manager and the import
// symbols, not from a text search). Whole-program rules are stored apart and
// reused only when every input is unchanged. A corrupt or mismatched entry
// is ignored and the run is done from scratch.

#include <filesystem>
#include <optional>
#include <ostream>
#include <string>
#include <string_view>
#include <vector>

#include "uvmcheck/core/Diagnostic.h"

namespace slang::ast {
class Compilation;
}

namespace uvmcheck::core::cache {

inline constexpr int kFormatVersion = 1;
inline constexpr std::string_view kToolVersion = "0.1.0";

/// True for a rule whose findings can change because some other file changed,
/// and that file is not an include or an imported package of the file that
/// holds the finding. Listed in docs/result_cache.md.
bool isWholeProgramRule(std::string_view ruleId);

struct EnvInput {
    std::vector<std::string> defines;
    std::vector<std::string> undefines;
    std::string languageVersion;
    bool disableLocalIncludes = false;
    bool incDirFirst = false;
    std::vector<std::string> includeDirs;
    /// Config, waiver, baseline, command files, and Python rule sources.
    std::vector<std::string> contentFiles;
    /// Flags that choose the rule set: preset, --rule, --guideline, engine, ...
    std::string flags;
};

/// Stable hash of everything in `in` plus the registered rule ids.
std::string environmentKey(const EnvInput& in);

struct Request {
    bool enabled = false;
    std::filesystem::path directory;
    std::string envKey;
    /// Canonical paths of the files about to be compiled, sorted unique.
    std::vector<std::string> primaries;
};

struct Hit {
    std::vector<Violation> violations;
    size_t compileErrors = 0;
    size_t waivedCount = 0;
};

/// Deletes the cache directory. Failures are ignored.
void clear(const std::filesystem::path& directory);

/// The complete cached result when every unit and the whole program match.
/// Nullopt on a miss, a version mismatch, or a corrupt file. Does not throw.
std::optional<Hit> loadComplete(const Request& req);

/// Writes the cache from a finished run. When some units still match the
/// previous entry, their per-file findings are taken from that entry and
/// `violations` is replaced with the mix. Whole-program findings are taken
/// from the previous entry only when every input is unchanged. Never throws.
void updateAndStore(const Request& req, slang::ast::Compilation& compilation,
                    std::vector<Violation>& violations, size_t compileErrors, size_t waivedCount,
                    std::ostream& info);

} // namespace uvmcheck::core::cache
