#pragma once

#include <string>
#include <vector>
#include <optional>
#include <span>
#include "slang/driver/Driver.h"
#include "uvmcheck/core/Diagnostic.h"
#include "uvmcheck/uvm/UvmModel.h"

namespace uvmcheck::core {

class LinterDriver {
public:
    LinterDriver();

    bool parseCommandLine(int argc, char** argv);
    int run();

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
    std::optional<std::string> changedSince;
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

    /// Loads the explicit or auto-discovered project config and CLI path
    /// filters. Returns false (after printing why) if the config is broken.
    bool loadProjectConfig();
};

} // namespace uvmcheck::core
