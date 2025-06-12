// Search functionality for NASCast
let searchIndex = null;
let flexSearchIndex = null;
let searchTimeout = null;

// Initialize search functionality
async function initSearch() {
    try {
        // Load search index
        const response = await fetch('search-index.json');
        searchIndex = await response.json();
        
        // Initialize FlexSearch
        flexSearchIndex = new FlexSearch.Index({
            tokenize: "forward",
            resolution: 9
        });

        // Add documents to FlexSearch index
        searchIndex.entries.forEach((entry, index) => {
            flexSearchIndex.add(index, `${entry.title} ${entry.meta}`);
        });

        console.log('Search index loaded successfully:', searchIndex.entries.length, 'entries');
    } catch (error) {
        console.error('Failed to load search index:', error);
    }
}

// Perform search
function performSearch(query) {
    if (!flexSearchIndex || !searchIndex || query.trim().length < 2) {
        hideSearchResults();
        return;
    }

    // Get search results from FlexSearch
    const results = flexSearchIndex.search(query, { limit: 20 });
    
    // Map results back to original entries
    const searchResults = results.map(index => searchIndex.entries[index]);
    
    // Group results by type
    const groupedResults = {
        movies: searchResults.filter(r => r.media_type === 'movie'),
        series: searchResults.filter(r => r.media_type === 'series'),
        episodes: searchResults.filter(r => r.media_type === 'episode')
    };

    displaySearchResults(groupedResults, query);
}

// Display search results
function displaySearchResults(results, query) {
    const resultsContainer = document.getElementById('search-results');
    
    const totalResults = results.movies.length + results.series.length + results.episodes.length;
    
    if (totalResults === 0) {
        resultsContainer.innerHTML = '<div class="search-no-results">No results found for "' + escapeHtml(query) + '"</div>';
        resultsContainer.classList.add('show');
        return;
    }

    let html = '';

    // Movies section
    if (results.movies.length > 0) {
        html += '<div class="search-section">';
        html += '<div class="search-section-title">Movies (' + results.movies.length + ')</div>';
        results.movies.forEach(movie => {
            html += createSearchResultItem(movie);
        });
        html += '</div>';
    }

    // TV Series section
    if (results.series.length > 0) {
        html += '<div class="search-section">';
        html += '<div class="search-section-title">TV Series (' + results.series.length + ')</div>';
        results.series.forEach(series => {
            html += createSearchResultItem(series);
        });
        html += '</div>';
    }

    // Episodes section
    if (results.episodes.length > 0) {
        html += '<div class="search-section">';
        html += '<div class="search-section-title">Episodes (' + results.episodes.length + ')</div>';
        results.episodes.slice(0, 10).forEach(episode => { // Limit episodes to 10 for performance
            html += createSearchResultItem(episode);
        });
        html += '</div>';
    }

    resultsContainer.innerHTML = html;
    resultsContainer.classList.add('show');
}

// Create individual search result item HTML
function createSearchResultItem(item) {
    const year = item.year ? ` (${item.year})` : '';
    const typeLabel = item.media_type === 'episode' ? 'Episode' : 
                     item.media_type === 'series' ? 'TV Series' : 'Movie';
    
    return `
        <a href="${escapeHtml(item.url)}" class="search-result-item">
            <img src="${escapeHtml(item.poster_url)}" alt="${escapeHtml(item.title)}" class="search-result-poster" 
                 onerror="this.src='https://via.placeholder.com/40x60.png?text=No+Poster'">
            <div class="search-result-info">
                <div class="search-result-title">${escapeHtml(item.title)}</div>
                <div class="search-result-meta">${typeLabel}${year}</div>
            </div>
        </a>
    `;
}

// Hide search results
function hideSearchResults() {
    const resultsContainer = document.getElementById('search-results');
    resultsContainer.classList.remove('show');
}

// Escape HTML to prevent XSS
function escapeHtml(text) {
    const div = document.createElement('div');
    div.textContent = text;
    return div.innerHTML;
}

// Debounce function
function debounce(func, wait) {
    return function executedFunction(...args) {
        const later = () => {
            clearTimeout(searchTimeout);
            func(...args);
        };
        clearTimeout(searchTimeout);
        searchTimeout = setTimeout(later, wait);
    };
}

// Initialize search when DOM is loaded
document.addEventListener('DOMContentLoaded', function() {
    const searchInput = document.getElementById('search-input');
    const searchClear = document.getElementById('search-clear');
    const searchResults = document.getElementById('search-results');

    if (!searchInput) return;

    // Initialize search index
    initSearch();

    // Debounced search function
    const debouncedSearch = debounce(performSearch, 300);

    // Search input event listener
    searchInput.addEventListener('input', function(e) {
        const query = e.target.value.trim();
        
        if (query.length > 0) {
            searchClear.style.display = 'flex';
            debouncedSearch(query);
        } else {
            searchClear.style.display = 'none';
            hideSearchResults();
        }
    });

    // Clear search
    searchClear.addEventListener('click', function() {
        searchInput.value = '';
        searchClear.style.display = 'none';
        hideSearchResults();
        searchInput.focus();
    });

    // Close search results when clicking outside
    document.addEventListener('click', function(e) {
        if (!e.target.closest('.search-container')) {
            hideSearchResults();
        }
    });

    // Handle escape key
    searchInput.addEventListener('keydown', function(e) {
        if (e.key === 'Escape') {
            hideSearchResults();
            searchInput.blur();
        }
    });

    // Keyboard shortcut (Cmd/Ctrl + K)
    document.addEventListener('keydown', function(e) {
        if ((e.metaKey || e.ctrlKey) && e.key === 'k') {
            e.preventDefault();
            searchInput.focus();
        }
    });
});
