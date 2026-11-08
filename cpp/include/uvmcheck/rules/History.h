#pragma once

// Version-control history for the SV-EVO rules: which commits touched which file, read
// from git. It is off unless --history is given, so a plain lint never spawns git and its
// results do not depend on the checkout.

#include <cstdint>
#include <string>
#include <unordered_map>
#include <vector>

namespace uvmcheck::rules::history {

void setEnabled(bool on);
bool enabled();

struct Log {
    std::string root;                  // repository top level, canonical
    std::vector<size_t> commitSizes;   // files touched by each commit, newest first
    /// Canonical absolute path -> indices into commitSizes of the commits that touched it.
    std::unordered_map<std::string, std::vector<uint32_t>> files;
};

/// The log of the git repository holding `path` (its last `maxCommits` non-merge commits),
/// or nullptr when history is off, git is missing or the file is not in a repository.
/// Logs are read once per repository and cached.
const Log* logFor(const std::string& path, size_t maxCommits);

/// `path` made absolute and canonical, the form Log::files uses.
std::string canonicalPath(const std::string& path);

} // namespace uvmcheck::rules::history
