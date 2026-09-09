import { invoke } from '@tauri-apps/api/core';
import { openUrl } from '@tauri-apps/plugin-opener';

const scanPath = document.getElementById('scanPath');
const scanBtn = document.getElementById('scanBtn');
const pickPathBtn = document.getElementById('pickPathBtn');
const scanStatus = document.getElementById('scanStatus');
const resultsSection = document.getElementById('resultsSection');
const packagesTableBody = document.getElementById('packagesTableBody');
const packageSearch = document.getElementById('packageSearch');
const sourceFilter = document.getElementById('sourceFilter');
const duplicatesContainer = document.getElementById('duplicatesContainer');
const cleanupDialog = document.getElementById('cleanupDialog');
const cleanupDetails = document.getElementById('cleanupDetails');
const cleanupConfirmation = document.getElementById('cleanupConfirmation');
const confirmationHint = document.getElementById('confirmationHint');
const confirmationInput = document.getElementById('confirmationInput');
const confirmCleanupBtn = document.getElementById('confirmCleanupBtn');
const installSearch = document.getElementById('installSearch');
const installSearchBtn = document.getElementById('installSearchBtn');
const installStatus = document.getElementById('installStatus');
const installResults = document.getElementById('installResults');
const installDialog = document.getElementById('installDialog');
const installDetails = document.getElementById('installDetails');
const installConfirmationHint = document.getElementById('installConfirmationHint');
const installConfirmationInput = document.getElementById('installConfirmationInput');
const confirmInstallBtn = document.getElementById('confirmInstallBtn');
const cancelInstallBtn = document.getElementById('cancelInstallBtn');
const checkUpdatesBtn = document.getElementById('checkUpdatesBtn');
const updateAllBtn = document.getElementById('updateAllBtn');
const updatesStatus = document.getElementById('updatesStatus');
const updatesResults = document.getElementById('updatesResults');
const updatesDialog = document.getElementById('updatesDialog');
const updatesDetails = document.getElementById('updatesDetails');
const updatesConfirmationHint = document.getElementById('updatesConfirmationHint');
const updatesConfirmationInput = document.getElementById('updatesConfirmationInput');
const confirmUpdatesBtn = document.getElementById('confirmUpdatesBtn');
const checkOrphansBtn = document.getElementById('checkOrphansBtn');
const cleanOrphansBtn = document.getElementById('cleanOrphansBtn');
const orphansStatus = document.getElementById('orphansStatus');
const orphansResults = document.getElementById('orphansResults');
const githubLink = document.getElementById('githubLink');
const tabDescription = document.getElementById('tabDescription');

let packages = [];
let duplicateGroups = [];
let sortColumn = 'name';
let sortAscending = true;
let cleanupPackage = null;
let installPackage = null;
let installResultsData = [];
let availableUpdates = [];
let orphanPackages = [];
let pendingOrphanRemoval = false;
const INSTALL_RESULT_LIMIT = 25;

githubLink.addEventListener('click', async () => {
  try {
    await openUrl('https://github.com/Marco-DeLao');
  } catch (error) {
    setScanStatus(`Could not open GitHub: ${error}`, 'error');
  }
});

function formatBytes(bytes) {
  if (!bytes) return '0 B';
  const units = ['B', 'KB', 'MB', 'GB', 'TB'];
  const unitIndex = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), units.length - 1);
  return `${(bytes / 1024 ** unitIndex).toFixed(unitIndex ? 2 : 0)} ${units[unitIndex]}`;
}

function setScanStatus(message, state = 'loading') {
  scanStatus.className = `scan-status visible ${state}`;
  scanStatus.textContent = message;
}

function setInstallStatus(message, state = 'loading') {
  installStatus.className = `scan-status visible ${state}`;
  installStatus.textContent = message;
}

function setUpdatesStatus(message, state = 'loading') {
  updatesStatus.className = `scan-status visible ${state}`;
  updatesStatus.textContent = message;
}

function setOrphansStatus(message, state = 'loading') {
  orphansStatus.className = `scan-status visible ${state}`;
  orphansStatus.textContent = message;
}

function renderSummary(result) {
  const duplicateBytes = result.wasted_size_bytes;
  const orphanBytes = orphanPackages.reduce((total, pkg) => total + pkg.size_bytes, 0);
  document.getElementById('totalPackages').textContent = result.all_packages.length;
  document.getElementById('totalSize').textContent = formatBytes(result.total_size_bytes);
  document.getElementById('duplicateCount').textContent = result.duplicate_groups.length;
  document.getElementById('wastedSpace').textContent = formatBytes(duplicateBytes + orphanBytes);
  document.getElementById('wastedSpaceBreakdown').textContent = `Duplicates: ${formatBytes(duplicateBytes)} · Orphans: ${formatBytes(orphanBytes)}`;
}

function filteredPackages() {
  const query = packageSearch.value.trim().toLowerCase();
  const source = sourceFilter.value.toLowerCase();

  return [...packages]
    .filter((pkg) => {
      const matchesQuery = !query || [pkg.name, pkg.package_id, pkg.version, pkg.install_path]
        .some((value) => value.toLowerCase().includes(query));
      return matchesQuery && (!source || pkg.source.toLowerCase() === source);
    })
    .sort((left, right) => {
      const leftValue = left[sortColumn];
      const rightValue = right[sortColumn];
      const comparison = typeof leftValue === 'number'
        ? leftValue - rightValue
        : String(leftValue).localeCompare(String(rightValue));
      return sortAscending ? comparison : -comparison;
    });
}

function renderPackages() {
  const visiblePackages = filteredPackages();
  packagesTableBody.replaceChildren();

  if (!visiblePackages.length) {
    packagesTableBody.innerHTML = '<tr><td colspan="6" class="no-results">No packages match this filter.</td></tr>';
    return;
  }

  for (const pkg of visiblePackages) {
    const row = document.createElement('tr');
    row.innerHTML = `
      <td>${escapeHtml(pkg.name)}</td>
      <td><span class="source-badge ${pkg.source.toLowerCase()}">${escapeHtml(pkg.source)}</span></td>
      <td>${escapeHtml(pkg.version)}</td>
      <td>${formatBytes(pkg.size_bytes)}</td>
      <td title="${escapeHtml(pkg.install_path)}">${escapeHtml(pkg.install_path)}</td>
      <td><button class="preview-btn" data-package-index="${packages.indexOf(pkg)}">Preview</button></td>`;
    packagesTableBody.appendChild(row);
  }
}

function packageIndexFor(packageToFind) {
  return packages.findIndex((pkg) => (
    pkg.package_id === packageToFind.package_id
    && pkg.source === packageToFind.source
    && pkg.install_path === packageToFind.install_path
  ));
}

async function openCleanupPreview(packageIndex) {
  try {
    cleanupPackage = packages[packageIndex];
    const preview = await invoke('cleanup_preview', { package: cleanupPackage });
    const safety = preview.safe_to_suggest
      ? '<p class="safe-message">This item is not classified as a core system package.</p>'
      : `<p class="blocked-message">${escapeHtml(preview.safety_reason)}</p>`;
    cleanupDetails.innerHTML = `
      <p><strong>${escapeHtml(preview.package_name)}</strong> (${escapeHtml(preview.source)})</p>
      <p>Would affect: <code>${escapeHtml(preview.install_path)}</code></p>
      <p>Space reported: <strong>${formatBytes(preview.size_bytes)}</strong></p>
      <p>Exact command preview: <code>${escapeHtml(preview.command)}</code></p>
      ${safety}
      <p class="preview-note">Removal requires exact confirmation, uses Polkit for privileged package operations, and records an audit entry locally.</p>`;
    cleanupConfirmation.classList.toggle('hidden', !preview.safe_to_suggest);
    confirmationInput.value = '';
    const requiredConfirmation = `REMOVE ${preview.package_name}`;
    confirmationHint.textContent = requiredConfirmation;
    confirmationInput.placeholder = requiredConfirmation;
    cleanupDialog.showModal();
  } catch (error) {
    setScanStatus(`Could not create cleanup preview: ${error}`, 'error');
  }
}

confirmCleanupBtn.addEventListener('click', async () => {
  if (!cleanupPackage) return;
  confirmCleanupBtn.disabled = true;
  try {
    const result = await invoke('cleanup_package', {
      package: cleanupPackage,
      confirmation: confirmationInput.value,
    });
    cleanupDialog.close();
    setScanStatus(`Removed ${result.package_name}; audit log: ${result.audit_log}`, 'success');
    await startScan();
  } catch (error) {
    setScanStatus(`Cleanup failed: ${error}`, 'error');
  } finally {
    confirmCleanupBtn.disabled = false;
  }
});

function renderInstallResults() {
  installResults.replaceChildren();
  if (!installResultsData.length) {
    installResults.innerHTML = '<div class="no-results">No installable packages found.</div>';
    return;
  }
  const visibleResults = installResultsData.slice(0, INSTALL_RESULT_LIMIT);
  const totalResults = installResultsData.length;
  if (totalResults > INSTALL_RESULT_LIMIT) {
    const notice = document.createElement('p');
    notice.className = 'install-result-count';
    notice.textContent = `${totalResults} total results, showing top ${INSTALL_RESULT_LIMIT} matches.`;
    installResults.appendChild(notice);
  }
  const grouped = new Map();
  for (const result of visibleResults) {
    if (!grouped.has(result.source)) grouped.set(result.source, []);
    grouped.get(result.source).push(result);
  }
  for (const [source, results] of grouped) {
    const section = document.createElement('section');
    section.className = 'install-source-group';
    section.innerHTML = `<h3>${escapeHtml(source)}</h3>${results.map((result) => `
      <article class="install-result">
        <div><strong>${escapeHtml(result.display_name)}</strong><span class="install-id">${escapeHtml(result.package_id)}</span><p>${escapeHtml(result.description || 'No description available.')}</p></div>
        <button type="button" class="install-btn" data-install-index="${installResultsData.indexOf(result)}">Install</button>
      </article>`).join('')}`;
    installResults.appendChild(section);
  }
}

async function searchInstallPackages() {
  const query = installSearch.value.trim();
  if (!query) return;
  installSearchBtn.disabled = true;
  setInstallStatus(`Searching available sources for ${query}...`);
  try {
    installResultsData = await invoke('search_packages', { query });
    renderInstallResults();
    const shown = Math.min(installResultsData.length, INSTALL_RESULT_LIMIT);
    setInstallStatus(installResultsData.length > shown
      ? `${installResultsData.length} results found; showing top ${shown} matches.`
      : `${installResultsData.length} results found.`, 'success');
  } catch (error) {
    setInstallStatus(`Package search failed: ${error}`, 'error');
  } finally {
    installSearchBtn.disabled = false;
  }
}

async function openInstallPreview(index) {
  installPackage = installResultsData[index];
  try {
    const preview = await invoke('install_preview', { package: installPackage });
    installDetails.innerHTML = `
      <p><strong>${escapeHtml(preview.package_name)}</strong> (${escapeHtml(preview.source)})</p>
      <p>Package ID: <code>${escapeHtml(preview.package_id)}</code></p>
      <p>Approximate size: <strong>${preview.approx_size ? formatBytes(preview.approx_size) : 'Not reported'}</strong></p>
      <p>Exact command: <code>${escapeHtml(preview.command)}</code></p>
      <p class="preview-note">The selected search result is revalidated before execution. Output is recorded in the audit log.</p>`;
    installConfirmationHint.textContent = preview.confirmation;
    installConfirmationInput.value = '';
    installDialog.showModal();
  } catch (error) {
    setInstallStatus(`Could not create install preview: ${error}`, 'error');
  }
}

confirmInstallBtn.addEventListener('click', async () => {
  if (!installPackage) return;
  confirmInstallBtn.disabled = true;
  cancelInstallBtn.classList.remove('hidden');
  setInstallStatus('Installing package. Full command output is being captured...', 'loading');
  try {
    const result = await invoke('install_package', { package: installPackage, confirmation: installConfirmationInput.value });
    installDialog.close();
    setInstallStatus(`Installed ${result.package_name}; audit log: ${result.audit_log}`, 'success');
    await startScan();
  } catch (error) {
    setInstallStatus(`Install failed: ${error}`, 'error');
  } finally {
    confirmInstallBtn.disabled = false;
    cancelInstallBtn.classList.add('hidden');
  }
});

cancelInstallBtn.addEventListener('click', async () => {
  cancelInstallBtn.disabled = true;
  await invoke('cancel_install');
  setInstallStatus('Cancellation requested; the package manager will finish cleanly.', 'loading');
});

function renderUpdates() {
  updatesResults.replaceChildren();
  updateAllBtn.classList.toggle('hidden', availableUpdates.length === 0);
  if (!availableUpdates.length) {
    updatesResults.innerHTML = '<div class="no-results">All packages are up to date. (0 updates available)</div>';
    return;
  }
  const table = document.createElement('div');
  table.className = 'updates-table';
  table.innerHTML = availableUpdates.map((item) => `
    <div class="update-row">
      <strong>${escapeHtml(item.display_name)}</strong>
      <span class="source-badge ${item.source.toLowerCase()}">${escapeHtml(item.source)}</span>
      <span>${escapeHtml(item.current_version)} &rarr; ${escapeHtml(item.available_version)}</span>
    </div>`).join('');
  updatesResults.appendChild(table);
}

async function checkForUpdates() {
  checkUpdatesBtn.disabled = true;
  setUpdatesStatus('Checking available updates. Pacman may ask Polkit to refresh package metadata...');
  try {
    const result = await invoke('check_updates');
    availableUpdates = result.updates;
    renderUpdates();
    const errors = result.errors.length ? ` ${result.errors.join(' ')}` : '';
    setUpdatesStatus(availableUpdates.length
      ? `${availableUpdates.length} updates available.${errors}`
      : 'Update check complete.', errors ? 'error' : 'success');
  } catch (error) {
    setUpdatesStatus(`Update check failed: ${error}`, 'error');
  } finally {
    checkUpdatesBtn.disabled = false;
  }
}

updateAllBtn.addEventListener('click', () => {
  const confirmation = `UPDATE ALL ${availableUpdates.length} PACKAGES`;
  const pacmanCount = availableUpdates.filter((item) => item.source === 'Pacman').length;
  const flatpakCount = availableUpdates.filter((item) => item.source === 'Flatpak').length;
  updatesDetails.innerHTML = `<p>This will update <strong>${availableUpdates.length}</strong> packages.</p><p>${pacmanCount} via Pacman; ${flatpakCount} via Flatpak.</p><p>Each package will be attempted separately and full output will be captured.</p>`;
  updatesConfirmationHint.textContent = confirmation;
  updatesConfirmationInput.value = '';
  updatesDialog.showModal();
});

confirmUpdatesBtn.addEventListener('click', async () => {
  confirmUpdatesBtn.disabled = true;
  try {
    if (pendingOrphanRemoval) {
      const outcomes = await invoke('remove_orphans', { packages: orphanPackages, confirmation: updatesConfirmationInput.value });
      updatesDialog.close();
      const failed = outcomes.filter((outcome) => !outcome.success);
      setOrphansStatus(`Orphan removal complete: ${outcomes.length - failed.length} removed, ${failed.length} failed.`, failed.length ? 'error' : 'success');
      await startScan();
      await checkForOrphans();
      return;
    }
    const report = await invoke('update_all', { packages: availableUpdates, confirmation: updatesConfirmationInput.value });
    updatesDialog.close();
    const failed = report.failed.length;
    const failureDetails = failed ? `<details><summary>Show failed packages</summary>${report.failed.map((item) => `<p><strong>${escapeHtml(item.package_id)}</strong>: ${escapeHtml(item.stderr || item.detail)}</p>`).join('')}</details>` : '';
    updatesResults.innerHTML = `<div class="update-report"><p>Updated successfully: ${report.successful.length} packages</p><p>Failed: ${failed} packages</p><p>Already up to date / skipped: ${report.skipped.length}</p>${failureDetails}</div>`;
    setUpdatesStatus(`Update run complete. Audit log: ${report.audit_log}`, failed ? 'error' : 'success');
    await startScan();
  } catch (error) {
    setUpdatesStatus(`Update failed: ${error}`, 'error');
  } finally {
    confirmUpdatesBtn.disabled = false;
    pendingOrphanRemoval = false;
    confirmUpdatesBtn.textContent = 'Confirm and update';
  }
});

checkUpdatesBtn.addEventListener('click', checkForUpdates);

function renderOrphans() {
  orphansResults.replaceChildren();
  cleanOrphansBtn.classList.toggle('hidden', orphanPackages.length === 0);
  if (!orphanPackages.length) {
    orphansResults.innerHTML = '<div class="no-results">No orphaned packages found.</div>';
    return;
  }
  orphansResults.innerHTML = orphanPackages.map((pkg) => `
    <div class="update-row"><strong>${escapeHtml(pkg.name)}</strong><span class="source-badge ${pkg.source.toLowerCase()}">${escapeHtml(pkg.source)}</span><span>${formatBytes(pkg.size_bytes)} - ${escapeHtml(pkg.note)}</span></div>`).join('');
}

async function checkForOrphans() {
  checkOrphansBtn.disabled = true;
  setOrphansStatus('Checking for orphaned dependencies...');
  try {
    const result = await invoke('check_orphans');
    orphanPackages = [...result.pacman, ...result.apt, ...result.dnf];
    if (packages.length) {
      renderSummary({
        all_packages: packages,
        total_size_bytes: packages.reduce((total, pkg) => total + pkg.size_bytes, 0),
        duplicate_groups: duplicateGroups,
        wasted_size_bytes: duplicateGroups.reduce((total, group) => total + group.wasted_size_bytes, 0),
      });
    }
    renderOrphans();
    const errors = result.errors.length ? ` ${result.errors.join(' ')}` : '';
    setOrphansStatus(orphanPackages.length ? `${orphanPackages.length} orphaned packages found.${errors}` : `No orphaned packages found.${errors}`, errors ? 'error' : 'success');
  } catch (error) {
    setOrphansStatus(`Orphan check failed: ${error}`, 'error');
  } finally {
    checkOrphansBtn.disabled = false;
  }
}

cleanOrphansBtn.addEventListener('click', () => {
  const confirmation = `REMOVE ${orphanPackages.length} ORPHANS`;
  updatesDetails.innerHTML = `<p>This will remove <strong>${orphanPackages.length}</strong> orphaned packages.</p><p>Total reported size: <strong>${formatBytes(orphanPackages.reduce((total, pkg) => total + pkg.size_bytes, 0))}</strong></p><p>Packages: <code>${escapeHtml(orphanPackages.map((pkg) => pkg.name).join(', '))}</code></p><p>Review the list carefully. Orphans can still be intentionally used directly.</p>`;
  updatesConfirmationHint.textContent = confirmation;
  updatesConfirmationInput.value = '';
  pendingOrphanRemoval = true;
  updatesDialog.showModal();
  confirmUpdatesBtn.textContent = 'Confirm and remove orphans';
});

updatesDialog.addEventListener('close', () => {
  pendingOrphanRemoval = false;
  confirmUpdatesBtn.textContent = 'Confirm and update';
});

checkOrphansBtn.addEventListener('click', checkForOrphans);

installSearchBtn.addEventListener('click', searchInstallPackages);
installSearch.addEventListener('keydown', (event) => { if (event.key === 'Enter') searchInstallPackages(); });
installResults.addEventListener('click', (event) => {
  const button = event.target.closest('.install-btn');
  if (button) openInstallPreview(Number(button.dataset.installIndex));
});

function renderDuplicates() {
  duplicatesContainer.replaceChildren();

  if (!duplicateGroups.length) {
    duplicatesContainer.innerHTML = '<div class="no-results"><h3>No duplicates found</h3><p>Packages from different sources will appear here when they match.</p></div>';
    return;
  }

  for (const group of duplicateGroups) {
    const card = document.createElement('article');
    card.className = 'duplicate-group';
    card.innerHTML = `
      <div class="duplicate-header" role="button" tabindex="0">
        <div>
          <div class="duplicate-title">${escapeHtml(group.app_name)} found in ${group.packages.length} places</div>
          <div class="duplicate-info">Potentially reclaimable: ${formatBytes(group.wasted_size_bytes)}</div>
        </div>
        <span class="duplicate-toggle">Show details</span>
      </div>
      <div class="duplicate-content">
        ${group.packages.map((pkg) => `
          <div class="duplicate-item">
            <span class="duplicate-item-name">${escapeHtml(pkg.name)}<br><small>${escapeHtml(pkg.version)}${group.version_comparison?.newer_package_id === pkg.package_id ? ' <strong class="newer-badge">Newer</strong>' : ''}</small></span>
            <span class="duplicate-item-source source-badge ${pkg.source.toLowerCase()}">${escapeHtml(pkg.source)}</span>
            <span class="duplicate-item-size">${formatBytes(pkg.size_bytes)}</span>
            <button class="duplicate-cleanup-btn" type="button" data-package-index="${packageIndexFor(pkg)}">Cleanup</button>
          </div>`).join('')}
        ${group.version_comparison ? `<p class="version-recommendation">${escapeHtml(group.version_comparison.recommendation)}</p>` : ''}`;
    const header = card.querySelector('.duplicate-header');
    const content = card.querySelector('.duplicate-content');
    const toggle = card.querySelector('.duplicate-toggle');
    const toggleDetails = () => {
      content.classList.toggle('expanded');
      toggle.textContent = content.classList.contains('expanded') ? 'Hide details' : 'Show details';
    };
    header.addEventListener('click', toggleDetails);
    header.addEventListener('keydown', (event) => {
      if (event.key === 'Enter' || event.key === ' ') toggleDetails();
    });
    duplicatesContainer.appendChild(card);
  }
}

function escapeHtml(value) {
  return String(value).replace(/[&<>'"]/g, (character) => ({
    '&': '&amp;', '<': '&lt;', '>': '&gt;', "'": '&#39;', '"': '&quot;'
  }[character]));
}

async function startScan() {
  scanBtn.disabled = true;
  setScanStatus(`Scanning ${scanPath.value || '/'}...`);
  try {
    const result = await invoke('start_scan', { scanPath: scanPath.value || '/' });
    packages = result.all_packages;
    duplicateGroups = result.duplicate_groups;
    try {
      const orphanResult = await invoke('check_orphans');
      orphanPackages = [...orphanResult.pacman, ...orphanResult.apt, ...orphanResult.dnf];
      renderOrphans();
    } catch (error) {
      orphanPackages = [];
      console.warn(`Could not refresh orphan space: ${error}`);
    }
    renderSummary(result);
    renderPackages();
    renderDuplicates();
    resultsSection.classList.remove('hidden');
    setScanStatus(`Scan complete: ${packages.length} packages found.`, 'success');
  } catch (error) {
    setScanStatus(`Scan failed: ${error}`, 'error');
  } finally {
    scanBtn.disabled = false;
  }
}

scanBtn.addEventListener('click', startScan);
pickPathBtn.addEventListener('click', async () => {
  pickPathBtn.disabled = true;
  try {
    const selectedPath = await invoke('pick_scan_path');
    scanPath.value = selectedPath;
    setScanStatus(`Selected scan path: ${selectedPath}`, 'success');
  } catch (error) {
    if (!String(error).toLowerCase().includes('cancel')) {
      setScanStatus(`Folder selection failed: ${error}`, 'error');
    }
  } finally {
    pickPathBtn.disabled = false;
  }
});
packagesTableBody.addEventListener('click', (event) => {
  const button = event.target.closest('.preview-btn');
  if (button) openCleanupPreview(Number(button.dataset.packageIndex));
});
duplicatesContainer.addEventListener('click', (event) => {
  const button = event.target.closest('.duplicate-cleanup-btn');
  if (button) openCleanupPreview(Number(button.dataset.packageIndex));
});
packageSearch.addEventListener('input', renderPackages);
sourceFilter.addEventListener('change', renderPackages);

document.querySelectorAll('.packages-table th').forEach((header, index) => {
  const columns = ['name', 'source', 'version', 'size_bytes', 'install_path'];
  header.addEventListener('click', () => {
    const nextColumn = columns[index];
    sortAscending = sortColumn === nextColumn ? !sortAscending : true;
    sortColumn = nextColumn;
    renderPackages();
  });
});

document.querySelectorAll('.tab-btn').forEach((button) => {
  button.addEventListener('click', () => {
    document.querySelectorAll('.tab-btn, .tab-content').forEach((element) => element.classList.remove('active'));
    button.classList.add('active');
    document.getElementById(button.dataset.tab).classList.add('active');
    tabDescription.textContent = {
      'all-packages': 'A complete list of every package installed on your system, from every source.',
      duplicates: 'Packages installed more than once through different sources, wasting disk space.',
      install: 'Search for and install new packages from Pacman, Flatpak, or other available sources.',
      updates: 'Check for and apply available updates across all your package sources.',
      orphans: 'Leftover dependency packages no longer needed by anything else on your system.',
    }[button.dataset.tab];
  });
});
