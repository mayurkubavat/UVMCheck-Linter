#include "uvmcheck/plugins/PythonPluginManager.h"
#include <fstream>
#include <iostream>
#include <sstream>
#include <cstdio>
#include <cstdlib>
#include <unistd.h>

namespace uvmcheck::plugins {

bool PythonPluginManager::runPythonRules(
    const std::string& rulesPath,
    const uvmcheck::uvm::UvmModel& uvmModel,
    core::DiagnosticEngine& diags) {

    // 1. Serialize semantic model to temporary file
    pid_t pid = getpid();
    std::string tempModelPath = "/tmp/uvmcheck_model_" + std::to_string(pid) + ".json";
    {
        std::ofstream out(tempModelPath);
        if (!out.is_open()) {
            std::cerr << "[uvmcheck-plugins] Error: Could not write temporary model file: " << tempModelPath << "\n";
            return false;
        }
        uvmModel.dumpSemanticJson(out);
    }

    // 2. Build execution command with PYTHONPATH pointing to python/ SDK
    std::string cmd = "PYTHONPATH=\"python:$PYTHONPATH\" python3 -m uvmcheck.plugin_runner "
                      "--rules \"" + rulesPath + "\" --model \"" + tempModelPath + "\" 2>&1";

    FILE* pipe = popen(cmd.c_str(), "r");
    if (!pipe) {
        std::cerr << "[uvmcheck-plugins] Error: Failed to execute python3 runner\n";
        unlink(tempModelPath.c_str());
        return false;
    }

    char buffer[1024];
    std::string output;
    while (fgets(buffer, sizeof(buffer), pipe) != nullptr) {
        output += buffer;
    }
    int retCode = pclose(pipe);
    unlink(tempModelPath.c_str());

    if (retCode != 0) {
        std::cerr << "[uvmcheck-plugins] Python plugin runner exited with error:\n" << output << "\n";
        return false;
    }

    // 3. Parse JSON output array
    size_t pos = 0;
    while ((pos = output.find('{', pos)) != std::string::npos) {
        size_t endObj = output.find('}', pos);
        if (endObj == std::string::npos) break;

        std::string obj = output.substr(pos, endObj - pos + 1);
        pos = endObj + 1;

        auto extractStr = [&](std::string_view key) -> std::string {
            size_t kpos = obj.find("\"" + std::string(key) + "\"");
            if (kpos == std::string::npos) return "";
            size_t colon = obj.find(':', kpos);
            if (colon == std::string::npos) return "";
            size_t q1 = obj.find('"', colon);
            if (q1 == std::string::npos || q1 >= obj.length()) return "";
            size_t q2 = obj.find('"', q1 + 1);
            if (q2 == std::string::npos) return "";
            return obj.substr(q1 + 1, q2 - q1 - 1);
        };

        auto extractInt = [&](std::string_view key) -> int {
            size_t kpos = obj.find("\"" + std::string(key) + "\"");
            if (kpos == std::string::npos) return 0;
            size_t colon = obj.find(':', kpos);
            if (colon == std::string::npos) return 0;
            size_t numStart = obj.find_first_of("0123456789", colon);
            if (numStart == std::string::npos) return 0;
            size_t numEnd = obj.find_first_not_of("0123456789", numStart);
            if (numEnd == std::string::npos) numEnd = obj.length();
            try {
                return std::stoi(obj.substr(numStart, numEnd - numStart));
            } catch (...) {
                return 0;
            }
        };

        std::string ruleId = extractStr("rule_id");
        if (ruleId.empty()) continue;

        std::string ruleName = extractStr("rule_name");
        std::string message = extractStr("message");
        std::string severityStr = extractStr("severity");
        std::string filename = extractStr("filename");
        int line = extractInt("line");
        int column = extractInt("column");

        core::Severity sev = core::Severity::Warning;
        if (severityStr == "Error") {
            sev = core::Severity::Error;
        } else if (severityStr == "Info") {
            sev = core::Severity::Info;
        }

        diags.addDirectViolation(
            ruleId,
            ruleName.empty() ? ruleId : ruleName,
            message,
            sev,
            filename,
            static_cast<size_t>(std::max(1, line)),
            static_cast<size_t>(std::max(1, column))
        );
    }

    return true;
}

} // namespace uvmcheck::plugins
