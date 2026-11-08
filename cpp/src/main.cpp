#include "uvmcheck/core/Driver.h"
#include "uvmcheck/core/Process.h"

#include <algorithm>
#include <cstdlib>
#include <functional>
#include <iostream>
#include <set>
#include <sstream>
#include <string>
#include <unistd.h>

namespace {

// --fix runs pass after pass until nothing more changes: a fix can uncover another
// finding, and overlapping fixes wait for the rewritten text. Each further pass is a
// fresh process (exec of this binary), so no state leaks between passes.
constexpr int kMaxFixPasses = 10;
constexpr const char* kPassEnv = "UVMCHECK_FIX_PASS";
constexpr const char* kStatesEnv = "UVMCHECK_FIX_STATES";

int currentPass() {
    const char* v = std::getenv(kPassEnv);
    return v ? std::max(1, std::atoi(v)) : 1;
}

// File states (path and content hash) written by earlier passes.
std::set<std::string> seenStates() {
    std::set<std::string> out;
    if (const char* v = std::getenv(kStatesEnv)) {
        std::stringstream list(v);
        for (std::string item; std::getline(list, item, ',');) {
            if (!item.empty()) out.insert(item);
        }
    }
    return out;
}

std::string stateKey(const std::pair<std::string, size_t>& written) {
    return std::to_string(std::hash<std::string>{}(written.first) ^ (written.second * 31));
}

} // namespace

int main(int argc, char** argv) {
    uvmcheck::core::LinterDriver driver;
    if (!driver.parseCommandLine(argc, argv)) {
        return 1;
    }
    if (!driver.rewritesFiles()) return driver.run();

    // Hold this pass's report back: if it rewrote files, the next pass reports on the
    // fixed text, and only its summary line is shown here.
    std::ostringstream report;
    auto* const stdoutBuf = std::cout.rdbuf(report.rdbuf());
    const int rc = driver.run();
    std::cout.rdbuf(stdoutBuf);

    const auto& result = driver.fixResult();
    const int pass = currentPass();
    if (result.changedFiles == 0) {
        std::cout << report.str();
        return rc;
    }

    auto seen = seenStates();
    bool cycled = true;
    for (const auto& w : result.written) {
        if (!seen.count(stateKey(w))) cycled = false;
    }
    if (cycled || pass >= kMaxFixPasses) {
        std::cout << report.str();
        std::cerr << "Warning: --fix: " << (cycled ? "fixes undo each other" : "fixes still changing files")
                  << " after " << pass << " pass(es); stopped. Review the remaining findings by hand.\n";
        return rc;
    }

    std::cout << result.summary << std::flush;
    for (const auto& w : result.written) seen.insert(stateKey(w));
    std::string states;
    for (const auto& s : seen) states += (states.empty() ? "" : ",") + s;
    setenv(kPassEnv, std::to_string(pass + 1).c_str(), 1);
    setenv(kStatesEnv, states.c_str(), 1);

    const std::string self = uvmcheck::core::selfExecutablePath(argv[0]);
    execv(self.c_str(), argv);
    std::cerr << "Warning: --fix: could not start pass " << pass + 1 << "; run --fix again to continue.\n";
    std::cout << report.str();
    return rc;
}
