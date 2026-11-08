    function toggleCode(id) {
      const el = document.getElementById(id);
      if (el.style.display === 'none') {
        el.style.display = 'block';
      } else {
        el.style.display = 'none';
      }
    }

    let currentSeverity = 'All';
    const searchInput = document.getElementById('searchInput');
    const rows = document.querySelectorAll('.violation-row');
    const filterButtons = document.querySelectorAll('.filter-btn');

    function applyFilters() {
      const q = searchInput.value.toLowerCase();
      rows.forEach(row => {
        const sev = row.getAttribute('data-sev');
        const text = row.innerText.toLowerCase();
        const matchesSev = (currentSeverity === 'All' || sev === currentSeverity);
        const matchesQuery = !q || text.includes(q);
        row.style.display = (matchesSev && matchesQuery) ? '' : 'none';
      });
    }

    searchInput.addEventListener('input', applyFilters);

    filterButtons.forEach(btn => {
      btn.addEventListener('click', () => {
        filterButtons.forEach(b => b.classList.remove('active'));
        btn.classList.add('active');
        currentSeverity = btn.getAttribute('data-sev');
        applyFilters();
      });
    });
