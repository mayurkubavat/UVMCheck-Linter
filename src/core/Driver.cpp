#include "uvmcheck/core/Driver.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "uvmcheck/rules/BuiltinRules.h"
#include "uvmcheck/waivers/WaiverEngine.h"
#include "uvmcheck/reporting/SarifReporter.h"
#include "uvmcheck/reporting/HtmlReporter.h"
#include "uvmcheck/reporting/JsonReporter.h"
#include "uvmcheck/reporting/Conformance.h"
#include "uvmcheck/rules/Guidelines.h"
#include "uvmcheck/rules/History.h"
#include "uvmcheck/core/ConfigManager.h"
#include "uvmcheck/plugins/PythonPluginManager.h"
#include "uvmcheck/lsp/LspServer.h"
#include "uvmcheck/core/ProjectSources.h"
#include "uvmcheck/core/ProjectInit.h"
#include "uvmcheck/core/ChangedLines.h"
#include "uvmcheck/reporting/TestbenchMap.h"
#include "uvmcheck/style/NamingEngine.h"
#include "uvmcheck/engine/RuleEngine.h"
#include <iostream>
#include <iomanip>
#include <fstream>
#include <set>
#include <unordered_set>
#include <algorithm>
#include <unordered_map>
#include <chrono>
#include <filesystem>

namespace fs = std::filesystem;

namespace uvmcheck::core {

LinterDriver::LinterDriver() {
    rules::registerBuiltinRules();

    slangDriver.addStandardArgs();

    slangDriver.cmdLine.add("-h,--help", showHelp, "Display available options");
    slangDriver.cmdLine.add("--version", showVersion, "Display version information and exit");
    slangDriver.cmdLine.add("--list-rules", listRules, "List all available linter rules and exit; with --json, write the rule catalog as JSON");
    slangDriver.cmdLine.add("--preset", presetName, "Select rule preset: soundness, methodology, performance, style, security, smells, advisory, reuse, guide, all", "<name>");
    slangDriver.cmdLine.add("--guideline", guidelineSelector, "Run only the rules that enforce these methodology guidelines or chapters (comma-separated, e.g. MG-4,MG-3.1)", "<ids>");
    slangDriver.cmdLine.add("--list-guidelines", listGuidelines, "List the methodology guide (guidelines and the rules that enforce them) and exit; with --json, write it as JSON");
    slangDriver.cmdLine.add("--conformance", showConformance, "Report pass/fail per methodology guideline (implied by --guideline and the reuse and guide presets)");
    slangDriver.cmdLine.add("--fix", autoFix, "Automatically apply fix-it rewrites to source files where supported");
    slangDriver.cmdLine.add("--sarif", sarifOutputPath, "Output results to specified SARIF 2.1.0 file", "<file>");
    slangDriver.cmdLine.add("--json", jsonOutputPath, "Output results as JSON to specified file ('-' for stdout)", "<file>");
    slangDriver.cmdLine.add("--html", htmlOutputPath, "Generate standalone interactive HTML quality dashboard", "<file>");
    slangDriver.cmdLine.add("--baseline", baselineInputPath, "Filter out known technical debt matching baseline JSON", "<file>");
    slangDriver.cmdLine.add("--update-baseline", baselineOutputPath, "Snapshot current violations into baseline JSON file", "<file>");
    slangDriver.cmdLine.add("--waivers", waiversFilePath, "Load external waiver rules from JSON file", "<file>");
    slangDriver.cmdLine.add("--config", configFilePath, "Load project configuration (Python .uvmcheck/config.py, uvmcheck_config.py, or JSON); default: auto-discover", "<file>");
    slangDriver.cmdLine.add("--dump-config", dumpConfigPath, "Write a starter Python project configuration and exit (e.g. .uvmcheck/config.py)", "<file>");
    slangDriver.cmdLine.add("--library", libraryPathArgs, "Treat files matching this glob as library code: compiled, never reported (repeatable)", "<glob>");
    slangDriver.cmdLine.add("--exclude", excludePathArgs, "Do not report violations in files matching this glob (repeatable)", "<glob>");
    slangDriver.cmdLine.add("--rule", ruleFilter, "Execute only the specified rule ID or rule name", "<rule>");
    slangDriver.cmdLine.add("--python-rules", pythonRulesPath, "Load and execute custom Python lint rules from specified file or directory", "<path>");
    slangDriver.cmdLine.add("--history", useHistory, "Read git history for the change-based rules (SV-EVO-001 hotspots, SV-EVO-002 change coupling); off by default");
    slangDriver.cmdLine.add("--profile-rules", profileRules, "Print per-phase and per-rule execution times to stderr");
    slangDriver.cmdLine.add("--engine", engineMode, "Rule execution engine: single-pass (default, parallel shared walk) or isolated (one walk per rule, sequential)", "<mode>");
    slangDriver.cmdLine.add("--waiver-report", waiverReport, "Report waiver and baseline hygiene: unused, ownerless and expired waivers, stale waiver comments, waived findings per rule and owner");
    slangDriver.cmdLine.add("--strict-waivers", strictWaivers, "Exit with status 4 when a waiver, waiver comment or baseline entry matched nothing, or a waiver has expired (implies --waiver-report)");
    slangDriver.cmdLine.add("--map", mapOutputPath, "Write the testbench map: component tree, TLM connections and config_db flows with findings attached (HTML, or JSON for a .json path)", "<file>");
    slangDriver.cmdLine.add("--changed-since", changedSince, "Report only findings on lines added or changed since this git revision (and in untracked files): a new-code-is-clean gate without a baseline", "<git-ref>");
    slangDriver.cmdLine.add("--no-config", noConfig, "Do not look for a project configuration (tests, one-off checks)");
    slangDriver.cmdLine.add("--init", initProject, "Set up this folder: find the sources and UVM, learn the naming conventions the code already follows, and write .uvmcheck/config.py and a first baseline");
    slangDriver.cmdLine.add("--lsp", runLsp, "Launch Language Server Protocol (LSP) daemon for IDE integration");
}

bool LinterDriver::parseCommandLine(int argc, char** argv) {
    return slangDriver.parseCommandLine(argc, argv);
}

bool LinterDriver::loadProjectConfig() {
    auto& cfgMgr = ConfigManager::instance();
    // Keep stdout parseable when the JSON report goes there.
    // (The language server owns stdout for JSON-RPC.)
    std::ostream& info = (jsonOutputPath.has_value() && *jsonOutputPath == "-") || runLsp.value_or(false)
                             ? std::cerr : std::cout;
    if (configFilePath.has_value()) {
        if (!cfgMgr.loadFromFile(*configFilePath)) {
            std::cerr << "Error: " << cfgMgr.getLastError() << "\n";
            return false;
        }
        info << "[uvmcheck] Loaded configuration from " << *configFilePath << "\n";
    } else if (!noConfig.value_or(false)) {
        // From the working directory up; failing that, from the first source file up, so a
        // run from a sim/ folder outside the tree still finds the project's .uvmcheck/.
        bool found = cfgMgr.autoDiscover(".");
        if (!found && cfgMgr.getLastError().empty() && slangDriver.sourceLoader.hasFiles()) {
            auto files = slangDriver.sourceLoader.getFilePaths();
            if (!files.empty()) found = cfgMgr.autoDiscover(fs::absolute(files.front()).parent_path().string());
        }
        if (found) {
            info << "[uvmcheck] Auto-discovered configuration: " << cfgMgr.getLoadedConfigPath() << "\n";
        } else if (!cfgMgr.getLastError().empty()) {
            std::cerr << "Error: " << cfgMgr.getLastError() << "\n";
            return false;
        }
    }
    if (cfgMgr.getLayers().size() > 1) {
        info << "[uvmcheck] Configuration layers:";
        for (const auto& l : cfgMgr.getLayers()) info << " " << l;
        info << "\n";
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
        std::error_code ec;
        fs::path p(*dumpConfigPath);
        if (p.has_parent_path()) {
            fs::create_directories(p.parent_path(), ec);
        }
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
        if (jsonOutputPath.has_value()) {
            if (!reporting::JsonReporter::writeRuleCatalog(*jsonOutputPath,
                                                           rules::RuleRegistry::instance().getRules())) {
                std::cerr << "Error: Failed to write rule catalog to: " << *jsonOutputPath << "\n";
                return 1;
            }
            return 0;
        }
        std::cout << "\n=== Registered UVMCheck Rules ===\n\n";
        std::cout << std::left << std::setw(15) << "Rule ID"
                  << std::setw(28) << "Name"
                  << std::setw(17) << "Category"
                  << std::setw(10) << "Severity"
                  << "Description\n";
        std::cout << std::string(118, '-') << "\n";

        for (const auto& rule : rules::RuleRegistry::instance().getRules()) {
            std::cout << std::left << std::setw(15) << rule->getId()
                      << std::setw(28) << rule->getName()
                      << std::setw(17) << rule->getCategory()
                      << std::setw(10) << severityToString(rule->getDefaultSeverity())
                      << rule->getDescription() << "\n";
        }
        std::cout << "\n";
        return 0;
    }

    if (listGuidelines.value_or(false)) {
        if (jsonOutputPath.has_value()) {
            if (!reporting::JsonReporter::writeGuidelines(*jsonOutputPath)) {
                std::cerr << "Error: Failed to write guidelines to: " << *jsonOutputPath << "\n";
                return 1;
            }
            return 0;
        }
        std::string_view chapter;
        for (const auto& g : rules::methodologyGuidelines()) {
            if (g.chapter != chapter) {
                chapter = g.chapter;
                std::cout << "\n" << g.chapter << " " << g.chapterTitle << "\n";
            }
            std::cout << "  " << std::left << std::setw(8) << g.id << std::setw(8)
                      << rules::levelName(g.level) << g.statement << "\n";
            const auto& ids = rules::rulesForGuideline(g.id);
            std::cout << "  " << std::string(16, ' ') << "Rules: ";
            if (ids.empty()) std::cout << "(review only)";
            for (size_t i = 0; i < ids.size(); ++i) std::cout << (i ? ", " : "") << ids[i];
            std::cout << "\n";
        }
        std::cout << "\n";
        return 0;
    }

    const bool initMode = initProject.value_or(false);
    init::Discovery discovery;
    if (initMode) {
        // A fresh project: ignore any config further up the tree.
        std::error_code ec;
        for (const char* existing : {".uvmcheck/config.py", ".uvmcheck/config.json", "uvmcheck_config.py"}) {
            if (fs::exists(existing, ec)) {
                std::cerr << "Error: " << existing << " already exists; remove it to run --init again\n";
                return 2;
            }
        }
        const bool cliFiles = slangDriver.sourceLoader.hasFiles();
        discovery = init::discover(".", cliFiles ? slangDriver.sourceLoader.getFilePaths()
                                                 : std::vector<fs::path>{});
        if (!cliFiles && !discovery.build.hasSources()) {
            std::cerr << "Error: no SystemVerilog sources found under " << discovery.root.string() << "\n";
            return 2;
        }
        std::cout << "[uvmcheck] init: " << discovery.svFiles << " SystemVerilog files; "
                  << (discovery.filelistsRel.empty() ? std::to_string(discovery.sourcesRel.size()) + " top-level sources"
                                                     : std::to_string(discovery.filelistsRel.size()) + " file list(s)")
                  << "; " << discovery.uvmNote << "\n";
        if (!applyBuildSettings(slangDriver, discovery.build, !cliFiles)) return 2;
    } else {
        if (!loadProjectConfig()) {
            return 2;
        }

        if (runLsp.value_or(false)) {
            lsp::LspServer server;
            return server.run();
        }

        // Sources from the project config, when the command line names none.
        const auto& build = ConfigManager::instance().getProjectConfig().build();
        const bool useConfigSources = !slangDriver.sourceLoader.hasFiles() && build.hasSources();
        if (!applyBuildSettings(slangDriver, build, useConfigSources)) return 2;
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

    std::vector<init::NamingChoice> learntNaming;
    if (initMode) {
        // Learn the naming conventions, write the config, then lint with it.
        fs::path libraryDir;
        if (!discovery.libraryRel.empty()) {
            std::string expanded;
            if (expandEnvVars(discovery.libraryRel.front(), expanded)) {
                fs::path p(expanded);
                libraryDir = (p.is_relative() ? discovery.root / p : p).lexically_normal();
            }
        }
        learntNaming = init::inferNaming(uvmModel, ConfigManager::instance().getNamingEngine(), libraryDir);
        std::error_code ec;
        fs::create_directories(".uvmcheck", ec);
        {
            std::ofstream out(".uvmcheck/config.py");
            out << init::renderConfig(discovery, learntNaming);
            if (!out) {
                std::cerr << "Error: cannot write .uvmcheck/config.py\n";
                return 2;
            }
        }
        configFilePath = ".uvmcheck/config.py";
        if (!loadProjectConfig()) return 2;
        if (!baselineOutputPath.has_value()) baselineOutputPath = ".uvmcheck/baseline.json";
    }
    double elaborateSeconds = std::chrono::duration<double>(Clock::now() - elaborateStart).count();

    // Initialize diagnostic engine
    DiagnosticEngine diags(slangDriver.sourceManager);

    const auto& projectConfig = ConfigManager::instance().getProjectConfig();
    diags.setProjectConfig(&projectConfig);

    // Resolve active rule preset profile
    // Advisory rules only run when asked for by preset or by --rule.
    uint32_t activeProfileMask = static_cast<uint32_t>(rules::RuleProfile::All) &
                                 ~static_cast<uint32_t>(rules::RuleProfile::Advisory);
    // Guideline selection (--guideline, or the reuse and guide presets) picks rules by the
    // guidelines they enforce, whatever their profile.
    std::vector<std::string> guidelineSelectors;
    if (guidelineSelector.has_value()) {
        std::string_view rest = *guidelineSelector;
        while (!rest.empty()) {
            size_t comma = rest.find(',');
            std::string_view item = rest.substr(0, comma);
            while (!item.empty() && item.front() == ' ') item.remove_prefix(1);
            while (!item.empty() && item.back() == ' ') item.remove_suffix(1);
            if (!item.empty()) guidelineSelectors.emplace_back(item);
            if (comma == std::string_view::npos) break;
            rest.remove_prefix(comma + 1);
        }
        for (const auto& sel : guidelineSelectors) {
            bool known = false;
            for (const auto& g : rules::methodologyGuidelines()) known |= rules::guidelineMatches(sel, g.id);
            if (!known) {
                std::cerr << "Error: Unknown guideline '" << sel << "' (see --list-guidelines)\n";
                return 2;
            }
        }
    }
    if (presetName.has_value()) {
        std::string p = *presetName;
        for (char& c : p) c = static_cast<char>(std::tolower(static_cast<unsigned char>(c)));
        if (p == "reuse") {
            guidelineSelectors.insert(guidelineSelectors.end(), {"MG-1", "MG-2", "MG-3", "MG-4", "MG-5"});
        } else if (p == "guide") {
            for (const auto& g : rules::methodologyGuidelines()) guidelineSelectors.emplace_back(g.id);
        } else if (p == "soundness") {
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
        } else if (p == "advisory") {
            activeProfileMask = static_cast<uint32_t>(rules::RuleProfile::Advisory);
        } else if (p == "all") {
            activeProfileMask = static_cast<uint32_t>(rules::RuleProfile::All);
        } else {
            std::cerr << "Warning: Unknown preset '" << *presetName << "', using 'all'.\n";
        }
    }

    engine::EngineOptions engineOptions;
    engineOptions.profile = profileRules.value_or(false);
    rules::history::setEnabled(useHistory.value_or(false));
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
        // A rule named with --rule runs whatever its profile.
        bool selected = ruleFilter.has_value();
        if (!selected && !guidelineSelectors.empty()) {
            for (auto g : rule->getGuidelines()) {
                for (const auto& sel : guidelineSelectors) selected |= rules::guidelineMatches(sel, g);
            }
        } else if (!selected) {
            selected = (rule->getProfiles() & activeProfileMask) != 0;
        }
        if (selected) activeRules.push_back(rule.get());
    }

    engine::RuleEngine ruleEngine(*compilation, uvmModel, slangDriver.sourceManager, engineOptions);

    // Custom Python rules only read the UVM model, so they run alongside the built-in rules
    if (pythonRulesPath.has_value()) {
        info << "[uvmcheck] Executing custom Python rules from: " << *pythonRulesPath << "\n";
        ruleEngine.addExternalJob("python rules", [&](DiagnosticEngine& pyDiags) {
            plugins::PythonPluginManager::runPythonRules(*pythonRulesPath, *compilation, uvmModel, pyDiags);
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
    // Baseline paths, and the default waivers/baseline files, are relative to the project
    // root: the folder that holds .uvmcheck/ (or the loaded config file), else the working directory.
    fs::path projectRoot = ConfigManager::instance().getProjectRoot();
    if (projectRoot.empty()) projectRoot = fs::current_path();
    waiverEngine.setProjectRoot(projectRoot);
    // Shown and opened relative to the working directory when the root is (or is under) it.
    const fs::path uvmcheckDir = [&] {
        fs::path rel = fs::weakly_canonical(projectRoot).lexically_proximate(fs::weakly_canonical(fs::current_path()));
        return (rel == "." ? fs::path() : rel) / ".uvmcheck";
    }();
    for (const auto& cw : ConfigManager::instance().getWaivers()) {
        waivers::ExternalWaiver w;
        w.ruleId = cw.ruleId;
        w.line = cw.line;
        w.reason = cw.reason;
        w.glob = cw.path;
        w.owner = cw.owner;
        w.label = cw.label;
        w.group = cw.group;
        waiverEngine.addWaiver(std::move(w));
    }

    std::string waiversPathToLoad;
    if (waiversFilePath.has_value()) {
        waiversPathToLoad = *waiversFilePath;
    } else {
        std::error_code ec;
        if (fs::exists(uvmcheckDir / "waivers.json", ec)) {
            waiversPathToLoad = (uvmcheckDir / "waivers.json").generic_string();
        }
    }

    if (!waiversPathToLoad.empty()) {
        if (!waiverEngine.loadWaivers(waiversPathToLoad)) {
            std::cerr << "Warning: Could not load waiver file: " << waiverEngine.getLastError() << "\n";
        } else {
            info << "[uvmcheck] Loaded external waivers from " << waiversPathToLoad << "\n";
        }
    }

    std::string baselinePathToLoad;
    if (baselineInputPath.has_value()) {
        baselinePathToLoad = *baselineInputPath;
    } else {
        std::error_code ec;
        if (fs::exists(uvmcheckDir / "baseline.json", ec)) {
            baselinePathToLoad = (uvmcheckDir / "baseline.json").generic_string();
        }
    }

    if (!baselinePathToLoad.empty()) {
        if (!waiverEngine.loadBaseline(baselinePathToLoad)) {
            std::cerr << "Warning: Could not load baseline file: " << waiverEngine.getLastError() << "\n";
        } else {
            info << "[uvmcheck] Loaded baseline from " << baselinePathToLoad << "\n";
        }
    }

    if (baselineOutputPath.has_value()) {
        std::error_code ec;
        fs::path p(*baselineOutputPath);
        if (p.has_parent_path()) {
            fs::create_directories(p.parent_path(), ec);
        }
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
    if (size_t stale = waiverEngine.getStaleBaselineCount(); stale > 0 && !baselineOutputPath.has_value()) {
        info << "[uvmcheck] " << stale << " baseline entr" << (stale == 1 ? "y" : "ies")
             << " matched no finding (fixed, or the flagged line changed); "
                "run with --update-baseline to drop them\n";
    }

    if (changedSince.has_value()) {
        ChangedLines changed;
        if (!changed.load(*changedSince, projectRoot)) {
            std::cerr << "Error: --changed-since: " << changed.error() << "\n";
            return 2;
        }
        size_t before = activeViolations.size();
        std::erase_if(activeViolations, [&](const Violation& v) {
            return v.filePath.empty() || !changed.contains(v.filePath, v.line);
        });
        info << "[uvmcheck] --changed-since " << *changedSince << ": " << activeViolations.size()
             << " finding(s) on changed lines in " << changed.fileCount() << " changed file(s); "
             << before - activeViolations.size() << " elsewhere not shown\n";
    }

    // Replace violations with active ones
    diags.getViolations() = std::move(activeViolations);

    bool hygieneFailed = false;
    if (waiverReport.value_or(false) || strictWaivers.value_or(false)) {
        // User files of this compilation: library and excluded files are not scanned for comments.
        std::vector<std::string> userFiles;
        std::set<std::string> seenFiles;
        for (const auto& tree : slangDriver.syntaxTrees) {
            for (auto id : tree->getSourceBufferIds()) {
                std::string name(slangDriver.sourceManager.getRawFileName(id));
                if (name.empty() || !seenFiles.insert(name).second) continue;
                if (projectConfig.classifyPath(normalizePathForMatching(name)) != SuppressionReason::None) continue;
                if (name.find("uvm_mock.svh") != std::string::npos) continue;
                userFiles.push_back(name);
            }
        }
        std::unordered_set<std::string> activeIds;
        for (const auto* r : activeRules) activeIds.insert(std::string(r->getId()));
        auto h = waiverEngine.hygiene(userFiles, activeIds);
        const auto& expired = ConfigManager::instance().getExpiredWaivers();
        hygieneFailed = !h.unused.empty() || !h.unusedInline.empty() || h.staleBaseline > 0 || !expired.empty();

        std::ostream& rep = jsonToStdout ? std::cerr : std::cout;
        rep << "\n=== Waiver hygiene ===\n";
        rep << "Waivers: " << h.written << " written, " << h.unused.size() << " matched nothing, "
            << expired.size() << " expired, " << h.withoutOwner << " without an owner\n";
        for (const auto& u : h.unused) rep << "  unused:  " << u << "\n";
        for (const auto& e : expired) rep << "  expired: " << e << "\n";
        rep << "Waiver comments: " << h.inlineComments << " in source, " << h.unusedInline.size()
            << " waive nothing\n";
        for (const auto& u : h.unusedInline) {
            rep << "  " << u.filePath << ":" << u.line << ": " << u.text << "\n";
        }
        rep << "Baseline: " << h.baselineEntries << " entr" << (h.baselineEntries == 1 ? "y" : "ies") << ", "
            << h.staleBaseline << " no longer match (fixed or changed)\n";
        if (!h.byRule.empty()) {
            rep << "Waived findings by rule:";
            for (size_t i = 0; i < h.byRule.size() && i < 10; ++i)
                rep << (i ? ", " : " ") << h.byRule[i].first << " " << h.byRule[i].second;
            rep << (h.byRule.size() > 10 ? ", ..." : "") << "\n";
            rep << "Waived findings by owner:";
            for (size_t i = 0; i < h.byOwner.size(); ++i)
                rep << (i ? ", " : " ") << h.byOwner[i].first << " " << h.byOwner[i].second;
            rep << "\n";
        }
        if (strictWaivers.value_or(false) && hygieneFailed) {
            rep << "[uvmcheck] --strict-waivers: remove or fix the entries above (exit 4)\n";
        }
    }

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

    std::vector<reporting::GuidelineResult> conformance;
    if (showConformance.value_or(false) || !guidelineSelectors.empty()) {
        conformance = reporting::computeConformance(activeRules, diags.getViolations());
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
        if (!reporting::JsonReporter::writeJson(*jsonOutputPath, diags.getViolations(), compileErrors, waivedCount,
                                                   conformance)) {
            std::cerr << "Error: Failed to write JSON report to: " << *jsonOutputPath << "\n";
        } else if (!jsonToStdout) {
            info << "[uvmcheck] JSON report written to: " << *jsonOutputPath << "\n";
        }
    }

    if (mapOutputPath.has_value()) {
        if (reporting::writeTestbenchMap(*mapOutputPath, *compilation, uvmModel, diags.getViolations())) {
            info << "[uvmcheck] Testbench map written to: " << *mapOutputPath << "\n";
        } else {
            std::cerr << "Error: Failed to write testbench map to: " << *mapOutputPath << "\n";
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

    if (initMode) {
        std::cout << "\n[uvmcheck] Wrote .uvmcheck/config.py";
        if (learntNaming.empty()) {
            std::cout << " (the default naming conventions already fit this code)";
        }
        std::cout << "\n";
        for (const auto& n : learntNaming) {
            std::cout << "  naming " << n.target << ": "
                      << (n.disable ? "disabled" : "pattern " + n.pattern) << " (" << n.evidence << ")\n";
        }
        std::cout << "[uvmcheck] Existing findings are in the baseline " << *baselineOutputPath
                  << "; from now on only new findings are reported.\n"
                  << "Review the config, then run `uvmcheck` from this folder.\n";
        return 0;
    }

    // Print summary
    if (!jsonToStdout) {
        diags.printSummary(std::cout);
        reporting::printConformance(std::cout, conformance);
    }

    if (waivedCount > 0) {
        info << "\033[2m[uvmcheck] " << waivedCount
                  << " violation(s) suppressed by inline waivers or baseline.\033[0m\n";
    }

    if (diags.errorCount() > 0) return 1;
    return strictWaivers.value_or(false) && hygieneFailed ? 4 : 0;
}

} // namespace uvmcheck::core
