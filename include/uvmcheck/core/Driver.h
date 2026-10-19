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
    slang::driver::Driver slangDriver;
    std::optional<bool> showHelp;
    std::optional<bool> showVersion;
    std::optional<bool> listRules;
    std::optional<std::string> sarifOutputPath;
    std::optional<std::string> htmlOutputPath;
    std::optional<std::string> baselineInputPath;
    std::optional<std::string> baselineOutputPath;
    std::optional<std::string> waiversFilePath;
    std::optional<std::string> presetName;
    std::optional<bool> autoFix;
    std::optional<std::string> configFilePath;
    std::optional<std::string> dumpConfigPath;
    std::optional<std::string> ruleFilter;
    std::optional<std::string> pythonRulesPath;
    std::optional<bool> runLsp;
};

} // namespace uvmcheck::core
