#include "uvmcheck/core/Driver.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "uvmcheck/rules/BuiltinRules.h"
#include "uvmcheck/waivers/WaiverEngine.h"
#include "uvmcheck/reporting/SarifReporter.h"
#include "uvmcheck/reporting/HtmlReporter.h"
#include "uvmcheck/reporting/JsonReporter.h"
#include "uvmcheck/core/ConfigManager.h"
#include "uvmcheck/plugins/PythonPluginManager.h"
#include "uvmcheck/lsp/LspServer.h"
#include "uvmcheck/engine/RuleEngine.h"
#include <iostream>
#include <iomanip>
#include <fstream>
#include <algorithm>
#include <unordered_map>
#include <chrono>

namespace uvmcheck::core {

LinterDriver::LinterDriver() {
    rules::registerBuiltinRules();

    slangDriver.addStandardArgs();

    slangDriver.cmdLine.add("-h,--help", showHelp, "Display available options");
    slangDriver.cmdLine.add("--version", showVersion, "Display version information and exit");
    slangDriver.cmdLine.add("--list-rules", listRules, "List all available linter rules and exit");
    slangDriver.cmdLine.add("--preset", presetName, "Select rule preset: soundness, methodology, performance, style, security, all", "<name>");
    slangDriver.cmdLine.add("--fix", autoFix, "Automatically apply fix-it rewrites to source files where supported");
    slangDriver.cmdLine.add("--sarif", sarifOutputPath, "Output results to specified SARIF 2.1.0 file", "<file>");
    slangDriver.cmdLine.add("--json", jsonOutputPath, "Output results as JSON to specified file ('-' for stdout)", "<file>");
    slangDriver.cmdLine.add("--html", htmlOutputPath, "Generate standalone interactive HTML quality dashboard", "<file>");
    slangDriver.cmdLine.add("--baseline", baselineInputPath, "Filter out known technical debt matching baseline JSON", "<file>");
    slangDriver.cmdLine.add("--update-baseline", baselineOutputPath, "Snapshot current violations into baseline JSON file", "<file>");
    slangDriver.cmdLine.add("--waivers", waiversFilePath, "Load external waiver rules from JSON file", "<file>");
    slangDriver.cmdLine.add("--config", configFilePath, "Load project configuration (Python uvmcheck_config.py or JSON); default: auto-discover", "<file>");
    slangDriver.cmdLine.add("--dump-config", dumpConfigPath, "Write a starter Python project configuration and exit", "<file>");
    slangDriver.cmdLine.add("--library", libraryPathArgs, "Treat files matching this glob as library code: compiled, never reported (repeatable)", "<glob>");
    slangDriver.cmdLine.add("--exclude", excludePathArgs, "Do not report violations in files matching this glob (repeatable)", "<glob>");
    slangDriver.cmdLine.add("--rule", ruleFilter, "Execute only the specified rule ID or rule name", "<rule>");
    slangDriver.cmdLine.add("--python-rules", pythonRulesPath, "Load and execute custom Python lint rules from specified file or directory", "<path>");
    slangDriver.cmdLine.add("--profile-rules", profileRules, "Print per-phase and per-rule execution times to stderr");
    slangDriver.cmdLine.add("--engine", engineMode, "Rule execution engine: single-pass (default, parallel shared walk) or isolated (one walk per rule, sequential)", "<mode>");
    slangDriver.cmdLine.add("--lsp", runLsp, "Launch Language Server Protocol (LSP) daemon for IDE integration");
}

bool LinterDriver::parseCommandLine(int argc, char** argv) {
    return slangDriver.parseCommandLine(argc, argv);
}

bool LinterDriver::loadProjectConfig() {
    auto& cfgMgr = ConfigManager::instance();
    // Keep stdout parseable when the JSON report goes there.
    std::ostream& info = jsonOutputPath.has_value() && *jsonOutputPath == "-" ? std::cerr : std::cout;
    if (configFilePath.has_value()) {
        if (!cfgMgr.loadFromFile(*configFilePath)) {
            std::cerr << "Error: " << cfgMgr.getLastError() << "\n";
            return false;
        }
        info << "[uvmcheck] Loaded configuration from " << *configFilePath << "\n";
    } else if (cfgMgr.autoDiscover(".")) {
        info << "[uvmcheck] Auto-discovered configuration: " << cfgMgr.getLoadedConfigPath() << "\n";
    } else if (!cfgMgr.getLastError().empty()) {
        std::cerr << "Error: " << cfgMgr.getLastError() << "\n";
        return false;
    }

    // Command-line globs are relative to the working directory.
    auto& project = cfgMgr.getProjectConfig();
    for (const auto& g : libraryPathArgs) project.libraryPaths().add(PathPattern(g, "."));
    for (const auto& g : excludePathArgs) project.excludePaths().add(PathPattern(g, "."));

    std::unordered_map<std::string, std::string> nameToId;
    std::vector<std::string> ids;
    for (const auto& rule : rules::RuleRegistry::instance().getRules()) {
        nameToId.emplace(rule->getName(), rule->getId());
        ids.emplace_back(rule->getId());
    }
    cfgMgr.validateRuleKeys(nameToId, ids);

    for (const auto& w : cfgMgr.getWarnings()) {
        std::cerr << "Warning: " << (cfgMgr.hasLoadedConfig() ? cfgMgr.getLoadedConfigPath() + ": " : "")
                  << w << "\n";
    }
    return true;
}

int LinterDriver::run() {
    if (showHelp.value_or(false)) {
        std::cout << slangDriver.cmdLine.getHelpText("uvmcheck - Advanced SystemVerilog & UVM Linter") << "\n";
        return 0;
    }

    if (showVersion.value_or(false)) {
        std::cout << "uvmcheck version 0.1.0 (powered by Slang)\n";
        return 0;
    }

    if (dumpConfigPath.has_value()) {
        std::ofstream out(*dumpConfigPath);
        if (!out.is_open()) {
            std::cerr << "Error: Could not open file for writing config template: " << *dumpConfigPath << "\n";
            return 1;
        }
        ConfigManager::instance().dumpDefaultConfig(out);
        std::cout << "[uvmcheck] Default configuration template written to: " << *dumpConfigPath << "\n";
        return 0;
    }

    if (listRules.value_or(false)) {
        std::cout << "\n=== Registered UVMCheck Rules ===\n\n";
        std::cout << std::left << std::setw(15) << "Rule ID"
                  << std::setw(28) << "Name"
                  << std::setw(14) << "Category"
                  << std::setw(10) << "Severity"
                  << "Description\n";
        std::cout << std::string(115, '-') << "\n";

        for (const auto& rule : rules::RuleRegistry::instance().getRules()) {
            std::cout << std::left << std::setw(15) << rule->getId()
                      << std::setw(28) << rule->getName()
                      << std::setw(14) << rule->getCategory()
                      << std::setw(10) << severityToString(rule->getDefaultSeverity())
                      << rule->getDescription() << "\n";
        }
        std::cout << "\n";
        return 0;
    }

    if (!loadProjectConfig()) {
        return 2;
    }

    if (runLsp.value_or(false)) {
        lsp::LspServer server;
        return server.run();
    }

    if (!slangDriver.processOptions()) {
        return 2;
    }

    if (!slangDriver.parseAllSources()) {
        (void)slangDriver.reportDiagnostics(false);
        return 3;
    }

    using Clock = std::chrono::steady_clock;
    auto elaborateStart = Clock::now();
    auto compilation = slangDriver.createCompilation();
    
    // With JSON on stdout, keep stdout parseable by moving status lines to stderr
    const bool jsonToStdout = jsonOutputPath.has_value() && *jsonOutputPath == "-";
    std::ostream& info = jsonToStdout ? std::cerr : std::cout;

    // Check if there are compilation syntax/elaboration errors. Elaboration
    // diagnostics only reach the engine through reportCompilation.
    slangDriver.reportCompilation(*compilation, /* quiet */ true);
    (void)slangDriver.reportDiagnostics(/* quiet */ jsonToStdout);
    const size_t compileErrors = static_cast<size_t>(slangDriver.diagEngine.getNumErrors());

    // Fully elaborate up front. Rules then only read the AST, which is what
    // makes it safe to run them in parallel (same protocol as slang's own
    // analysis passes), and every rule sees the same, complete design.
    (void)compilation->getAllDiagnostics();

    // Construct UVM knowledge model
    uvm::UvmModel uvmModel(*compilation);
    double elaborateSeconds = std::chrono::duration<double>(Clock::now() - elaborateStart).count();

    // Initialize diagnostic engine
    DiagnosticEngine diags(slangDriver.sourceManager);

    const auto& projectConfig = ConfigManager::instance().getProjectConfig();
    diags.setProjectConfig(&projectConfig);

    // Resolve active rule preset profile
    uint32_t activeProfileMask = static_cast<uint32_t>(rules::RuleProfile::All);
    if (presetName.has_value()) {
        std::string p = *presetName;
        for (char& c : p) c = static_cast<char>(std::tolower(static_cast<unsigned char>(c)));
        if (p == "soundness") {
            activeProfileMask = static_cast<uint32_t>(rules::RuleProfile::Soundness);
        } else if (p == "methodology") {
            activeProfileMask = static_cast<uint32_t>(rules::RuleProfile::Methodology);
        } else if (p == "performance" || p == "perf") {
            activeProfileMask = static_cast<uint32_t>(rules::RuleProfile::Performance);
        } else if (p == "style") {
            activeProfileMask = static_cast<uint32_t>(rules::RuleProfile::Style);
        } else if (p == "security") {
            activeProfileMask = static_cast<uint32_t>(rules::RuleProfile::Security);
        } else if (p == "smells" || p == "smell" || p == "maintainability") {
            activeProfileMask = static_cast<uint32_t>(rules::RuleProfile::Smells);
        } else if (p == "all") {
            activeProfileMask = static_cast<uint32_t>(rules::RuleProfile::All);
        } else {
            std::cerr << "Warning: Unknown preset '" << *presetName << "', using 'all'.\n";
        }
    }

    engine::EngineOptions engineOptions;
    engineOptions.profile = profileRules.value_or(false);
    engineOptions.threads = slangDriver.options.numThreads.value_or(0);
    if (engineMode.has_value()) {
        if (*engineMode == "isolated") {
            engineOptions.mode = engine::ExecutionMode::Isolated;
        } else if (*engineMode != "single-pass") {
            std::cerr << "Error: Unknown --engine '" << *engineMode << "' (expected single-pass or isolated)\n";
            return 2;
        }
    }

    // Select registered rules matching active profile and filter
    const auto& allRules = rules::RuleRegistry::instance().getRules();
    std::vector<rules::Rule*> activeRules;
    for (const auto& rule : allRules) {
        if (ruleFilter.has_value()) {
            if (rule->getId() != *ruleFilter && rule->getName() != *ruleFilter) {
                continue;
            }
        }
        if (!projectConfig.isRuleEnabled(rule->getId(), rule->getName())) {
            continue;
        }
        if ((rule->getProfiles() & activeProfileMask) != 0) {
            activeRules.push_back(rule.get());
        }
    }

    engine::RuleEngine ruleEngine(*compilation, uvmModel, slangDriver.sourceManager, engineOptions);

    // Custom Python rules only read the UVM model, so they run alongside the built-in rules
    if (pythonRulesPath.has_value()) {
        info << "[uvmcheck] Executing custom Python rules from: " << *pythonRulesPath << "\n";
        ruleEngine.addExternalJob("python rules", [&](DiagnosticEngine& pyDiags) {
            plugins::PythonPluginManager::runPythonRules(*pythonRulesPath, uvmModel, pyDiags);
        });
    }

    ruleEngine.run(activeRules, diags);

    if (engineOptions.profile) {
        std::cerr << "[profile] elaborate + UVM model: " << std::fixed << std::setprecision(3)
                  << elaborateSeconds << " s\n";
        ruleEngine.printProfile(std::cerr);
    }

    // Process Waivers and Baseline
    waivers::WaiverEngine waiverEngine(slangDriver.sourceManager);
    for (const auto& cw : ConfigManager::instance().getWaivers()) {
        waivers::ExternalWaiver w;
        w.ruleId = cw.ruleId;
        w.line = cw.line;
        w.reason = cw.reason;
        w.glob = cw.path;
        waiverEngine.addWaiver(std::move(w));
    }

    if (waiversFilePath.has_value()) {
        if (!waiverEngine.loadWaivers(*waiversFilePath)) {
            std::cerr << "Warning: Could not open waiver file: " << *waiversFilePath << "\n";
        } else {
            info << "[uvmcheck] Loaded external waivers from " << *waiversFilePath << "\n";
        }
    }

    if (baselineInputPath.has_value()) {
        if (!waiverEngine.loadBaseline(*baselineInputPath)) {
            std::cerr << "Warning: Could not open baseline file: " << *baselineInputPath << "\n";
        } else {
            info << "[uvmcheck] Loaded baseline from " << *baselineInputPath << "\n";
        }
    }

    if (baselineOutputPath.has_value()) {
        if (waiverEngine.saveBaseline(*baselineOutputPath, diags.getViolations())) {
            info << "[uvmcheck] Successfully saved " << diags.getViolations().size()
                      << " violations to baseline: " << *baselineOutputPath << "\n";
        } else {
            std::cerr << "Error: Failed to write baseline to: " << *baselineOutputPath << "\n";
        }
    }

    // Filter violations (applies inline waivers + external waivers + baseline suppressions)
    auto activeViolations = waiverEngine.filter(diags.getViolations());
    size_t waivedCount = waiverEngine.getWaivedCount();

    // Replace violations with active ones
    diags.getViolations() = std::move(activeViolations);

    // Apply automated fix-its if requested
    if (autoFix.value_or(false)) {
        std::unordered_map<std::string, std::vector<FixItHint>> fileFixes;
        for (const auto& v : diags.getViolations()) {
            for (const auto& fix : v.fixIts) {
                if (fix.range.start().valid() && !v.filePath.empty()) {
                    fileFixes[v.filePath].push_back(fix);
                }
            }
        }

        size_t totalFixed = 0;
        for (const auto& [path, fixes] : fileFixes) {
            std::ifstream in(path, std::ios::binary);
            if (!in.is_open()) continue;
            std::string content((std::istreambuf_iterator<char>(in)), std::istreambuf_iterator<char>());
            in.close();

            auto sortedFixes = fixes;
            std::sort(sortedFixes.begin(), sortedFixes.end(), [](const FixItHint& a, const FixItHint& b) {
                return a.range.start().offset() > b.range.start().offset();
            });

            bool modified = false;
            for (const auto& fix : sortedFixes) {
                size_t startOffset = fix.range.start().offset();
                size_t endOffset = fix.range.end().offset();
                if (startOffset <= endOffset && endOffset <= content.size()) {
                    content.replace(startOffset, endOffset - startOffset, fix.replacementText);
                    modified = true;
                    totalFixed++;
                }
            }

            if (modified) {
                std::ofstream out(path, std::ios::binary | std::ios::trunc);
                if (out.is_open()) {
                    out << content;
                    out.close();
                }
            }
        }

        if (totalFixed > 0) {
            info << "[uvmcheck] Automatically applied " << totalFixed << " fix-it(s) across "
                      << fileFixes.size() << " file(s).\n";
        }
    }

    // Write SARIF report if requested
    if (sarifOutputPath.has_value()) {
        if (reporting::SarifReporter::writeSarif(*sarifOutputPath, diags.getViolations(), allRules)) {
            info << "[uvmcheck] SARIF 2.1.0 report written to: " << *sarifOutputPath << "\n";
        } else {
            std::cerr << "Error: Failed to write SARIF report to: " << *sarifOutputPath << "\n";
        }
    }

    // Write JSON report if requested
    if (jsonOutputPath.has_value()) {
        if (!reporting::JsonReporter::writeJson(*jsonOutputPath, diags.getViolations(), compileErrors, waivedCount)) {
            std::cerr << "Error: Failed to write JSON report to: " << *jsonOutputPath << "\n";
        } else if (!jsonToStdout) {
            info << "[uvmcheck] JSON report written to: " << *jsonOutputPath << "\n";
        }
    }

    // Write HTML quality dashboard if requested
    if (htmlOutputPath.has_value()) {
        if (reporting::HtmlReporter::writeHtml(*htmlOutputPath, diags.getViolations(), allRules)) {
            info << "[uvmcheck] Interactive HTML dashboard generated: " << *htmlOutputPath << "\n";
        } else {
            std::cerr << "Error: Failed to write HTML report to: " << *htmlOutputPath << "\n";
        }
    }

    // Print summary
    if (!jsonToStdout) {
        diags.printSummary(std::cout);
    }

    if (waivedCount > 0) {
        info << "\033[2m[uvmcheck] " << waivedCount
                  << " violation(s) suppressed by inline waivers or baseline.\033[0m\n";
    }

    return diags.errorCount() > 0 ? 1 : 0;
}

} // namespace uvmcheck::core
