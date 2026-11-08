#pragma once

#include <string>
#include <vector>
#include <optional>
#include <span>
#include "slang/driver/Driver.h"
#include "uvmcheck/core/Diagnostic.h"
#include "uvmcheck/uvm/UvmModel.h"
#include "uvmcheck/compat/Overlay.h"

namespace uvmcheck::core {

/// What one --fix pass wrote, so main() can run another pass on the rewritten files.
struct FixPassResult {
    size_t applied = 0;
    size_t changedFiles = 0;
    /// Each rewritten file with a hash of its new text, to notice fixes that undo each other.
    std::vector<std::pair<std::string, size_t>> written;
    std::string summary; ///< the "[uvmcheck] Applied ..." line
};

class LinterDriver {
public:
    LinterDriver();

    bool parseCommandLine(int argc, char** argv);
    int run();

    /// True for --fix (not --fix-dry-run): the run rewrites source files.
    bool rewritesFiles() const { return autoFix.value_or(false) && !autoFixDryRun.value_or(false); }
    const FixPassResult& fixResult() const { return fixPass; }

private:
    int explain(const std::string& rule) const;
    slang::driver::Driver slangDriver;
    std::optional<bool> showHelp;
    std::optional<bool> showVersion;
    std::optional<bool> listRules;
    std::optional<std::string> sarifOutputPath;
    std::optional<std::string> htmlOutputPath;
    std::optional<std::string> jsonOutputPath;
    std::optional<std::string> baselineInputPath;
    std::optional<std::string> baselineOutputPath;
    std::optional<std::string> waiversFilePath;
    std::optional<std::string> presetName;
    std::optional<bool> autoFix;
    std::optional<bool> autoFixDryRun;
    std::optional<bool> unsafeFixes;
    std::optional<bool> includeExperimental;
    std::optional<std::string> autoFixRules;
    std::optional<std::string> configFilePath;
    std::optional<std::string> dumpConfigPath;
    std::optional<std::string> ruleFilter;
    std::optional<std::string> pythonRulesPath;
    std::optional<bool> runLsp;
    std::optional<bool> initProject;
    std::optional<bool> noConfig;
    std::optional<bool> noCache;
    std::optional<bool> clearCache;
    std::optional<std::string> cacheDirArg;
    std::vector<std::string> commandArgs;
    std::optional<std::string> changedSince;
    std::optional<bool> strictNewFiles;
    std::optional<std::string> mapOutputPath;
    std::optional<std::string> explainRule;
    std::optional<bool> waiverReport;
    std::optional<bool> strictWaivers;
    std::optional<bool> reportUnusedWaivers;
    std::vector<std::string> libraryPathArgs;
    std::vector<std::string> excludePathArgs;
    std::optional<bool> profileRules;
    std::optional<bool> useHistory;
    std::optional<std::string> engineMode;
    std::optional<std::string> guidelineSelector;
    std::optional<bool> listGuidelines;
    std::optional<bool> showConformance;
    std::optional<std::string> compatProfile;
    std::vector<std::string> overrideArgs;
    std::optional<bool> listOverrides;
    compat::VendorArgs vendorArgs;
    compat::Overlay overlay;
    FixPassResult fixPass;

    /// Loads the explicit or auto-discovered project config and CLI path
    /// filters. Returns false (after printing why) if the config is broken.
    bool loadProjectConfig();
};

} // namespace uvmcheck::core
