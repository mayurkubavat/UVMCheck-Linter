#include "uvmcheck/reporting/HtmlReporter.h"
#include <fstream>
#include <sstream>
#include <unordered_map>
#include <unordered_set>
#include <iomanip>
#include <chrono>
#include <ctime>

namespace uvmcheck::reporting {

std::string HtmlReporter::escapeHtml(std::string_view str) {
    std::string out;
    out.reserve(str.size() * 1.2);
    for (char c : str) {
        switch (c) {
            case '&':  out += "&amp;"; break;
            case '<':  out += "&lt;"; break;
            case '>':  out += "&gt;"; break;
            case '"':  out += "&quot;"; break;
            case '\'': out += "&#39;"; break;
            default:   out += c; break;
        }
    }
    return out;
}

static std::string getCategory(std::string_view ruleId) {
    if (ruleId.starts_with("UVM-")) return "UVM";
    if (ruleId.starts_with("SV-OOP-") || ruleId.starts_with("SV-OCP-") || ruleId.starts_with("SV-ENC-")) return "OOP";
    if (ruleId.starts_with("SV-THR-") || ruleId.starts_with("SV-INT-") || ruleId.starts_with("SV-CB-") ||
        ruleId.starts_with("SV-FOR-") || ruleId.starts_with("SV-RCE-") || ruleId.starts_with("SV-EVT-")) return "Concurrency";
    if (ruleId.starts_with("SV-PRF-") || ruleId.starts_with("SV-LOC-") || ruleId.starts_with("SV-ASC-") || ruleId.starts_with("SV-ARR-")) return "Performance";
    if (ruleId.starts_with("SV-STY-") || ruleId.starts_with("SV-NAM-")) return "Style";
    if (ruleId.starts_with("SV-DED-")) return "DeadCode";
    if (ruleId.starts_with("SV-SML-")) return "Smell";
    if (ruleId.starts_with("SV-COR-") || ruleId.starts_with("SV-DLY-") || ruleId.starts_with("SV-STR-") || ruleId.starts_with("SV-LANG-")) return "Correctness";
    if (ruleId.starts_with("SV-COV-")) return "Coverage";
    if (ruleId.starts_with("SV-RND-")) return "Random";
    if (ruleId.starts_with("SEC-"))    return "Security";
    if (ruleId.starts_with("PORT-"))   return "Portability";
    if (ruleId.starts_with("DPI-"))    return "DPI";
    if (ruleId.starts_with("SVA-"))    return "SVA";
    if (ruleId.starts_with("RAL-"))    return "RAL";
    return "General";
}

bool HtmlReporter::writeHtml(const std::string& outputPath,
                             const std::vector<core::Violation>& violations,
                             const std::vector<std::unique_ptr<rules::Rule>>& rules) {
    std::ofstream out(outputPath);
    if (!out.is_open()) return false;

    // Metrics aggregation
    size_t errorCount = 0;
    size_t warningCount = 0;
    size_t infoCount = 0;
    size_t totalDebtMinutes = 0;
    std::unordered_map<std::string, size_t> categoryCounts;
    std::unordered_map<std::string, size_t> ruleCounts;
    std::unordered_set<std::string> uniqueFiles;

    for (const auto& v : violations) {
        if (v.severity == core::Severity::Error) errorCount++;
        else if (v.severity == core::Severity::Warning) warningCount++;
        else infoCount++;

        if (v.ruleId == "SV-SML-004") totalDebtMinutes += 60;
        else if (v.ruleId == "SV-SML-002") totalDebtMinutes += 30;
        else if (v.ruleId == "SV-SML-001") totalDebtMinutes += 20;
        else if (v.ruleId == "SV-PRF-007" || v.ruleId == "SV-PRF-008") totalDebtMinutes += 5;
        else if (v.ruleId == "SV-PRF-005") totalDebtMinutes += 15;
        else if (v.ruleId == "SV-PRF-009") totalDebtMinutes += 10;
        else if (v.severity == core::Severity::Error) totalDebtMinutes += 45;
        else if (v.severity == core::Severity::Warning) totalDebtMinutes += 15;
        else totalDebtMinutes += 5;

        categoryCounts[getCategory(v.ruleId)]++;
        ruleCounts[v.ruleId]++;
        uniqueFiles.insert(v.filePath);
    }

    double totalDebtHours = static_cast<double>(totalDebtMinutes) / 60.0;
    std::string grade = "A";
    std::string gradeColor = "#10b981";
    if (totalDebtHours > 20.0) { grade = "E"; gradeColor = "#ef4444"; }
    else if (totalDebtHours > 10.0) { grade = "D"; gradeColor = "#f97316"; }
    else if (totalDebtHours > 5.0)  { grade = "C"; gradeColor = "#eab308"; }
    else if (totalDebtHours > 2.0)  { grade = "B"; gradeColor = "#3b82f6"; }

    // Timestamp
    auto now = std::chrono::system_clock::now();
    std::time_t now_c = std::chrono::system_clock::to_time_t(now);
    std::tm tm_buf;
#if defined(_WIN32)
    localtime_s(&tm_buf, &now_c);
#else
    localtime_r(&now_c, &tm_buf);
#endif
    char timeStr[64];
    std::strftime(timeStr, sizeof(timeStr), "%Y-%m-%d %H:%M:%S", &tm_buf);

    out << "<!DOCTYPE html>\n"
        << "<html lang=\"en\">\n"
        << "<head>\n"
        << "  <meta charset=\"UTF-8\">\n"
        << "  <meta name=\"viewport\" content=\"width=device-width, initial-scale=1.0\">\n"
        << "  <title>UVMCheck Quality Dashboard</title>\n"
        << "  <style>\n"
        << "    :root {\n"
        << "      --bg: #0b0f19;\n"
        << "      --surface: #131b2e;\n"
        << "      --surface-border: #1e293b;\n"
        << "      --text: #f1f5f9;\n"
        << "      --text-muted: #94a3b8;\n"
        << "      --err: #f43f5e;\n"
        << "      --err-bg: rgba(244, 63, 94, 0.12);\n"
        << "      --warn: #f59e0b;\n"
        << "      --warn-bg: rgba(245, 158, 11, 0.12);\n"
        << "      --info: #38bdf8;\n"
        << "      --info-bg: rgba(56, 189, 248, 0.12);\n"
        << "      --accent: #6366f1;\n"
        << "      --code-bg: #070a12;\n"
        << "    }\n"
        << "    * { box-sizing: border-box; margin: 0; padding: 0; font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Helvetica, Arial, sans-serif; }\n"
        << "    body { background-color: var(--bg); color: var(--text); padding: 2rem; line-height: 1.5; }\n"
        << "    .container { max-width: 1400px; margin: 0 auto; }\n"
        << "    header { display: flex; justify-content: space-between; align-items: center; border-bottom: 1px solid var(--surface-border); padding-bottom: 1.5rem; margin-bottom: 2rem; }\n"
        << "    .logo-group h1 { font-size: 1.75rem; font-weight: 700; letter-spacing: -0.025em; display: flex; align-items: center; gap: 0.5rem; }\n"
        << "    .badge-pill { font-size: 0.75rem; font-weight: 600; padding: 0.2rem 0.6rem; border-radius: 9999px; background: var(--accent); color: white; }\n"
        << "    .meta-time { color: var(--text-muted); font-size: 0.875rem; }\n"
        << "    .grid-kpi { display: grid; grid-template-columns: repeat(auto-fit, minmax(220px, 1fr)); gap: 1rem; margin-bottom: 2rem; }\n"
        << "    .card { background: var(--surface); border: 1px solid var(--surface-border); border-radius: 0.75rem; padding: 1.25rem; transition: transform 0.15s ease; }\n"
        << "    .card:hover { transform: translateY(-2px); }\n"
        << "    .card-title { font-size: 0.8125rem; font-weight: 600; text-transform: uppercase; letter-spacing: 0.05em; color: var(--text-muted); margin-bottom: 0.5rem; }\n"
        << "    .card-val { font-size: 2rem; font-weight: 800; }\n"
        << "    .val-err { color: var(--err); }\n"
        << "    .val-warn { color: var(--warn); }\n"
        << "    .val-info { color: var(--info); }\n"
        << "    .analytics-section { display: grid; grid-template-columns: 2fr 1fr; gap: 1.5rem; margin-bottom: 2rem; }\n"
        << "    @media (max-width: 900px) { .analytics-section { grid-template-columns: 1fr; } }\n"
        << "    .bar-row { display: flex; align-items: center; margin-bottom: 0.75rem; gap: 1rem; font-size: 0.875rem; }\n"
        << "    .bar-label { width: 120px; font-weight: 500; color: var(--text); }\n"
        << "    .bar-track { flex: 1; height: 10px; background: var(--surface-border); border-radius: 5px; overflow: hidden; }\n"
        << "    .bar-fill { height: 100%; background: linear-gradient(90deg, #6366f1, #38bdf8); border-radius: 5px; }\n"
        << "    .bar-count { width: 40px; text-align: right; color: var(--text-muted); font-weight: 600; }\n"
        << "    .filter-bar { background: var(--surface); border: 1px solid var(--surface-border); border-radius: 0.75rem; padding: 1rem; display: flex; flex-wrap: wrap; gap: 1rem; align-items: center; justify-content: space-between; margin-bottom: 1.5rem; }\n"
        << "    .search-box { flex: 1; min-width: 280px; }\n"
        << "    .search-box input { width: 100%; background: #070a12; border: 1px solid var(--surface-border); border-radius: 0.5rem; padding: 0.6rem 1rem; color: var(--text); font-size: 0.875rem; outline: none; }\n"
        << "    .search-box input:focus { border-color: var(--accent); }\n"
        << "    .btn-group { display: flex; gap: 0.5rem; }\n"
        << "    .filter-btn { background: #070a12; border: 1px solid var(--surface-border); color: var(--text-muted); padding: 0.5rem 0.875rem; border-radius: 0.5rem; font-size: 0.8125rem; font-weight: 600; cursor: pointer; transition: all 0.15s ease; }\n"
        << "    .filter-btn.active, .filter-btn:hover { background: var(--accent); color: white; border-color: var(--accent); }\n"
        << "    table { width: 100%; border-collapse: collapse; background: var(--surface); border: 1px solid var(--surface-border); border-radius: 0.75rem; overflow: hidden; font-size: 0.875rem; }\n"
        << "    th { background: #162036; padding: 0.875rem 1rem; text-align: left; font-weight: 600; color: var(--text-muted); border-bottom: 1px solid var(--surface-border); }\n"
        << "    td { padding: 1rem; border-bottom: 1px solid var(--surface-border); vertical-align: top; }\n"
        << "    tr:last-child td { border-bottom: none; }\n"
        << "    tr:hover td { background: rgba(255, 255, 255, 0.015); }\n"
        << "    .sev-pill { display: inline-flex; align-items: center; padding: 0.2rem 0.55rem; border-radius: 0.375rem; font-size: 0.75rem; font-weight: 700; text-transform: uppercase; letter-spacing: 0.05em; }\n"
        << "    .sev-Error { background: var(--err-bg); color: var(--err); border: 1px solid rgba(244, 63, 94, 0.3); }\n"
        << "    .sev-Warning { background: var(--warn-bg); color: var(--warn); border: 1px solid rgba(245, 158, 11, 0.3); }\n"
        << "    .sev-Info { background: var(--info-bg); color: var(--info); border: 1px solid rgba(56, 189, 248, 0.3); }\n"
        << "    .rule-id { font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace; font-weight: 700; color: #a5b4fc; }\n"
        << "    .loc-text { color: var(--text-muted); font-size: 0.8125rem; font-family: ui-monospace, Menlo, monospace; }\n"
        << "    .code-snippet { background: var(--code-bg); border: 1px solid var(--surface-border); border-radius: 0.375rem; padding: 0.5rem 0.75rem; margin-top: 0.5rem; font-family: ui-monospace, Menlo, Monaco, Consolas, monospace; font-size: 0.8125rem; white-space: pre-wrap; overflow-x: auto; color: #cbd5e1; }\n"
        << "    .toggle-code-btn { background: none; border: none; color: #818cf8; font-size: 0.75rem; font-weight: 600; cursor: pointer; margin-top: 0.35rem; padding: 0; text-decoration: underline; }\n"
        << "  </style>\n"
        << "</head>\n"
        << "<body>\n"
        << "  <div class=\"container\">\n"
        << "    <header>\n"
        << "      <div class=\"logo-group\">\n"
        << "        <h1>UVMCheck Quality Dashboard <span class=\"badge-pill\">v0.2.0</span></h1>\n"
        << "        <div class=\"meta-time\">SystemVerilog &amp; UVM Static Analysis &bull; Generated " << timeStr << "</div>\n"
        << "      </div>\n"
        << "    </header>\n\n";

    // KPI Cards
    std::ostringstream debtStr;
    debtStr << std::fixed << std::setprecision(1) << totalDebtHours;

    out << "    <div class=\"grid-kpi\">\n"
        << "      <div class=\"card\">\n"
        << "        <div class=\"card-title\">Total Violations</div>\n"
        << "        <div class=\"card-val\">" << violations.size() << "</div>\n"
        << "      </div>\n"
        << "      <div class=\"card\">\n"
        << "        <div class=\"card-title\">Maintainability Rating</div>\n"
        << "        <div class=\"card-val\" style=\"color: " << gradeColor << ";\">Grade " << grade << "</div>\n"
        << "      </div>\n"
        << "      <div class=\"card\">\n"
        << "        <div class=\"card-title\">Technical Debt</div>\n"
        << "        <div class=\"card-val\">" << debtStr.str() << " <span style=\"font-size: 0.9rem; color: var(--text-muted); font-weight: normal;\">hrs</span></div>\n"
        << "      </div>\n"
        << "      <div class=\"card\">\n"
        << "        <div class=\"card-title\">Errors / Warnings</div>\n"
        << "        <div class=\"card-val\"><span class=\"val-err\">" << errorCount << "</span> / <span class=\"val-warn\">" << warningCount << "</span></div>\n"
        << "      </div>\n"
        << "      <div class=\"card\">\n"
        << "        <div class=\"card-title\">Files Checked</div>\n"
        << "        <div class=\"card-val val-info\">" << uniqueFiles.size() << "</div>\n"
        << "      </div>\n"
        << "    </div>\n\n";

    // Category Breakdown Chart
    out << "    <div class=\"analytics-section\">\n"
        << "      <div class=\"card\">\n"
        << "        <div class=\"card-title\">Violations by Category</div>\n";

    size_t maxCategoryVal = 1;
    for (const auto& [cat, count] : categoryCounts) {
        if (count > maxCategoryVal) maxCategoryVal = count;
    }

    static const std::vector<std::string> standardCategories = {"UVM", "Performance", "Smell", "Concurrency", "Style", "OOP", "Security", "Portability"};
    for (const auto& cat : standardCategories) {
        size_t count = categoryCounts[cat];
        double pct = (static_cast<double>(count) / static_cast<double>(maxCategoryVal)) * 100.0;
        out << "        <div class=\"bar-row\">\n"
            << "          <span class=\"bar-label\">" << cat << "</span>\n"
            << "          <div class=\"bar-track\"><div class=\"bar-fill\" style=\"width: " << pct << "%\"></div></div>\n"
            << "          <span class=\"bar-count\">" << count << "</span>\n"
            << "        </div>\n";
    }

    out << "      </div>\n"
        << "      <div class=\"card\">\n"
        << "        <div class=\"card-title\">Top Rules Flagged</div>\n";

    // Top Rules
    std::vector<std::pair<std::string, size_t>> sortedRules(ruleCounts.begin(), ruleCounts.end());
    std::sort(sortedRules.begin(), sortedRules.end(), [](const auto& a, const auto& b) {
        return a.second > b.second;
    });

    size_t shownRules = 0;
    for (const auto& [rId, cnt] : sortedRules) {
        if (++shownRules > 5) break;
        out << "        <div style=\"display:flex; justify-content:space-between; margin-bottom: 0.5rem; font-size: 0.875rem;\">\n"
            << "          <span class=\"rule-id\">" << rId << "</span>\n"
            << "          <span style=\"font-weight:600;\">" << cnt << "</span>\n"
            << "        </div>\n";
    }
    if (sortedRules.empty()) {
        out << "        <div style=\"color: var(--text-muted); font-size: 0.875rem;\">No violations detected!</div>\n";
    }

    out << "      </div>\n"
        << "    </div>\n\n";

    // Filter controls
    out << "    <div class=\"filter-bar\">\n"
        << "      <div class=\"search-box\">\n"
        << "        <input type=\"text\" id=\"searchInput\" placeholder=\"Search rules, files, descriptions...\">\n"
        << "      </div>\n"
        << "      <div class=\"btn-group\">\n"
        << "        <button class=\"filter-btn active\" data-sev=\"All\">All (" << violations.size() << ")</button>\n"
        << "        <button class=\"filter-btn\" data-sev=\"Error\">Errors (" << errorCount << ")</button>\n"
        << "        <button class=\"filter-btn\" data-sev=\"Warning\">Warnings (" << warningCount << ")</button>\n"
        << "        <button class=\"filter-btn\" data-sev=\"Info\">Info (" << infoCount << ")</button>\n"
        << "      </div>\n"
        << "    </div>\n\n";

    // Violations Table
    out << "    <table id=\"violationsTable\">\n"
        << "      <thead>\n"
        << "        <tr>\n"
        << "          <th style=\"width: 100px;\">Severity</th>\n"
        << "          <th style=\"width: 130px;\">Rule ID</th>\n"
        << "          <th style=\"width: 180px;\">Category / Name</th>\n"
        << "          <th>Location &amp; Message</th>\n"
        << "        </tr>\n"
        << "      </thead>\n"
        << "      <tbody>\n";

    for (size_t i = 0; i < violations.size(); ++i) {
        const auto& v = violations[i];
        std::string sevStr = v.severity == core::Severity::Error ? "Error" :
                             v.severity == core::Severity::Warning ? "Warning" : "Info";

        out << "        <tr class=\"violation-row\" data-sev=\"" << sevStr << "\">\n"
            << "          <td><span class=\"sev-pill sev-" << sevStr << "\">" << sevStr << "</span></td>\n"
            << "          <td><span class=\"rule-id\">" << escapeHtml(v.ruleId) << "</span></td>\n"
            << "          <td>\n"
            << "            <div style=\"font-weight: 600;\">" << escapeHtml(v.ruleName) << "</div>\n"
            << "            <div style=\"color: var(--text-muted); font-size: 0.75rem;\">" << escapeHtml(getCategory(v.ruleId)) << "</div>\n"
            << "          </td>\n"
            << "          <td>\n"
            << "            <div class=\"loc-text\">" << escapeHtml(v.filePath) << ":" << v.line << ":" << v.column << "</div>\n"
            << "            <div style=\"margin-top: 0.25rem;\">" << escapeHtml(v.message) << "</div>\n";

        if (!v.snippet.empty()) {
            std::string codeId = "code_" + std::to_string(i);
            out << "            <button class=\"toggle-code-btn\" onclick=\"toggleCode('" << codeId << "')\">Show Snippet</button>\n"
                << "            <div class=\"code-snippet\" id=\"" << codeId << "\" style=\"display: none;\">"
                << escapeHtml(v.snippet) << "</div>\n";
        }

        out << "          </td>\n"
            << "        </tr>\n";
    }

    out << "      </tbody>\n"
        << "    </table>\n"
        << "  </div>\n\n";

    // JavaScript logic for instant search & filtering
    out << "  <script>\n"
        << "    function toggleCode(id) {\n"
        << "      const el = document.getElementById(id);\n"
        << "      if (el.style.display === 'none') {\n"
        << "        el.style.display = 'block';\n"
        << "      } else {\n"
        << "        el.style.display = 'none';\n"
        << "      }\n"
        << "    }\n\n"
        << "    let currentSeverity = 'All';\n"
        << "    const searchInput = document.getElementById('searchInput');\n"
        << "    const rows = document.querySelectorAll('.violation-row');\n"
        << "    const filterButtons = document.querySelectorAll('.filter-btn');\n\n"
        << "    function applyFilters() {\n"
        << "      const q = searchInput.value.toLowerCase();\n"
        << "      rows.forEach(row => {\n"
        << "        const sev = row.getAttribute('data-sev');\n"
        << "        const text = row.innerText.toLowerCase();\n"
        << "        const matchesSev = (currentSeverity === 'All' || sev === currentSeverity);\n"
        << "        const matchesQuery = !q || text.includes(q);\n"
        << "        row.style.display = (matchesSev && matchesQuery) ? '' : 'none';\n"
        << "      });\n"
        << "    }\n\n"
        << "    searchInput.addEventListener('input', applyFilters);\n\n"
        << "    filterButtons.forEach(btn => {\n"
        << "      btn.addEventListener('click', () => {\n"
        << "        filterButtons.forEach(b => b.classList.remove('active'));\n"
        << "        btn.classList.add('active');\n"
        << "        currentSeverity = btn.getAttribute('data-sev');\n"
        << "        applyFilters();\n"
        << "      });\n"
        << "    });\n"
        << "  </script>\n"
        << "</body>\n"
        << "</html>\n";

    return true;
}

} // namespace uvmcheck::reporting
