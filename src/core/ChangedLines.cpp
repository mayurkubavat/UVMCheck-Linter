#include "uvmcheck/core/ChangedLines.h"

#include "uvmcheck/core/Process.h"

#include <cstdlib>
#include <sstream>

namespace uvmcheck::core {

namespace fs = std::filesystem;

namespace {

std::string canonicalStr(const fs::path& p) {
    std::error_code ec;
    fs::path c = fs::weakly_canonical(fs::absolute(p, ec), ec);
    return (ec ? p.lexically_normal() : c).generic_string();
}

std::string trimEol(std::string s) {
    while (!s.empty() && (s.back() == '\n' || s.back() == '\r')) s.pop_back();
    return s;
}

} // namespace

void ChangedLines::parseDiff(const std::string& diff, const fs::path& root) {
    std::istringstream in(diff);
    std::string line, current;
    while (std::getline(in, line)) {
        if (line.rfind("+++ ", 0) == 0) {
            std::string path = line.substr(4);
            if (path == "/dev/null") {
                current.clear();
                continue;
            }
            if (path.rfind("b/", 0) == 0) path = path.substr(2);
            current = canonicalStr(root / path);
            ranges[current]; // a touched file, even if only lines were removed
        } else if (line.rfind("@@ ", 0) == 0 && !current.empty()) {
            // @@ -a[,b] +c[,d] @@
            size_t plus = line.find(" +");
            if (plus == std::string::npos) continue;
            size_t start = std::strtoul(line.c_str() + plus + 2, nullptr, 10);
            size_t count = 1;
            size_t comma = line.find(',', plus);
            size_t space = line.find(' ', plus + 2);
            if (comma != std::string::npos && comma < space) count = std::strtoul(line.c_str() + comma + 1, nullptr, 10);
            if (count > 0) ranges[current].emplace_back(start, start + count - 1);
        }
    }
}

bool ChangedLines::load(const std::string& ref, const fs::path& dir) {
    std::string out;
    if (runCapture({"git", "-C", dir.string(), "rev-parse", "--show-toplevel"}, {}, out) != 0) {
        lastError = "not inside a git repository: " + dir.string();
        return false;
    }
    fs::path root = trimEol(out);
    out.clear();
    if (runCapture({"git", "-C", root.string(), "rev-parse", "--verify", "--quiet", ref + "^{commit}"}, {}, out) != 0) {
        lastError = "unknown git revision '" + ref + "'";
        return false;
    }
    out.clear();
    if (runCapture({"git", "-c", "core.quotepath=off", "-C", root.string(), "diff", "--unified=0", "--no-color",
                    "--no-ext-diff", "--no-renames", ref, "--"},
                   {}, out) != 0) {
        lastError = "git diff " + ref + " failed";
        return false;
    }
    parseDiff(out, root);
    out.clear();
    if (runCapture({"git", "-c", "core.quotepath=off", "-C", root.string(), "ls-files", "--others",
                    "--exclude-standard"},
                   {}, out) == 0) {
        std::istringstream in(out);
        for (std::string f; std::getline(in, f);) {
            if (!f.empty()) wholeFiles.insert(canonicalStr(root / f));
        }
    }
    return true;
}

bool ChangedLines::contains(const std::string& filePath, size_t line) const {
    std::string p = canonicalStr(filePath);
    if (wholeFiles.count(p)) return true;
    auto it = ranges.find(p);
    if (it == ranges.end()) return false;
    for (const auto& [first, last] : it->second) {
        if (line >= first && line <= last) return true;
    }
    return false;
}

} // namespace uvmcheck::core
