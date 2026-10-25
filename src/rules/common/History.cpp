#include "uvmcheck/rules/History.h"

#include <atomic>
#include <cstdio>
#include <filesystem>
#include <map>
#include <memory>
#include <mutex>

namespace uvmcheck::rules::history {

namespace {

std::atomic<bool> historyOn{false};
std::mutex cacheMutex;
std::map<std::pair<std::string, size_t>, std::unique_ptr<Log>> logs;
std::map<std::string, std::string> rootOfDir; // directory -> repository root ("" for none)

std::string shellQuote(const std::string& s) {
    std::string out = "'";
    for (char c : s) {
        if (c == '\'') out += "'\\''";
        else out += c;
    }
    return out + "'";
}

bool runCommand(const std::string& cmd, std::string& out) {
    out.clear();
    FILE* pipe = popen((cmd + " 2>/dev/null").c_str(), "r");
    if (!pipe) return false;
    char buf[4096];
    size_t n;
    while ((n = fread(buf, 1, sizeof(buf), pipe)) > 0) out.append(buf, n);
    return pclose(pipe) == 0;
}

std::string repoRoot(const std::string& dir) {
    if (auto it = rootOfDir.find(dir); it != rootOfDir.end()) return it->second;
    std::string out;
    std::string root;
    if (runCommand("git -C " + shellQuote(dir) + " rev-parse --show-toplevel", out)) {
        while (!out.empty() && (out.back() == '\n' || out.back() == '\r')) out.pop_back();
        root = canonicalPath(out);
    }
    rootOfDir[dir] = root;
    return root;
}

std::unique_ptr<Log> readLog(const std::string& root, size_t maxCommits) {
    std::string out;
    std::string cmd = "git -c core.quotepath=off -C " + shellQuote(root) +
                      " log --no-merges --no-renames --format=@@%H --name-only -n " + std::to_string(maxCommits);
    if (!runCommand(cmd, out)) return nullptr;
    auto log = std::make_unique<Log>();
    log->root = root;
    size_t pos = 0;
    bool inCommit = false;
    while (pos < out.size()) {
        size_t end = out.find('\n', pos);
        if (end == std::string::npos) end = out.size();
        std::string line = out.substr(pos, end - pos);
        pos = end + 1;
        if (!line.empty() && line.back() == '\r') line.pop_back();
        if (line.rfind("@@", 0) == 0) {
            log->commitSizes.push_back(0);
            inCommit = true;
            continue;
        }
        if (line.empty() || !inCommit) continue;
        auto idx = static_cast<uint32_t>(log->commitSizes.size() - 1);
        log->commitSizes.back()++;
        std::filesystem::path p = std::filesystem::path(root) / line;
        log->files[p.lexically_normal().string()].push_back(idx);
    }
    return log;
}

} // namespace

void setEnabled(bool on) { historyOn = on; }
bool enabled() { return historyOn; }

std::string canonicalPath(const std::string& path) {
    std::error_code ec;
    auto p = std::filesystem::weakly_canonical(std::filesystem::absolute(path, ec), ec);
    return ec ? std::filesystem::path(path).lexically_normal().string() : p.string();
}

const Log* logFor(const std::string& path, size_t maxCommits) {
    if (!historyOn) return nullptr;
    std::lock_guard<std::mutex> lock(cacheMutex);
    std::string dir = std::filesystem::path(canonicalPath(path)).parent_path().string();
    std::string root = repoRoot(dir);
    if (root.empty()) return nullptr;
    auto key = std::make_pair(root, maxCommits);
    auto it = logs.find(key);
    if (it == logs.end()) it = logs.emplace(key, readLog(root, maxCommits)).first;
    return it->second.get();
}

} // namespace uvmcheck::rules::history
