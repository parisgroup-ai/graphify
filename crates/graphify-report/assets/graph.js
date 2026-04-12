// Graphify — Interactive Architecture Visualization
// Self-contained D3.js force-directed graph explorer.

(function () {
  'use strict';

  // ===========================================================================
  // Constants
  // ===========================================================================

  var CANVAS_THRESHOLD = 300;
  var MIN_R = 4;
  var MAX_R = 20;
  var EDGE_COLORS = { Imports: '#666', Defines: '#2196F3', Calls: '#4CAF50' };
  var CYCLE_COLOR = '#F44336';
  var EDGE_DASH = { Imports: null, Defines: '5,3', Calls: '2,2' };
  var COMMUNITY_COLORS = [
    '#8dd3c7','#ffffb3','#bebada','#fb8072','#80b1d3',
    '#fdb462','#b3de69','#fccde5','#d9d9d9','#bc80bd','#ccebc5','#ffed6f'
  ];

  // ===========================================================================
  // State
  // ===========================================================================

  var data = window.GRAPHIFY_DATA;
  var maxScore = 0.001;
  var maxWeight = 1;
  data.nodes.forEach(function (n) { if (n.score > maxScore) maxScore = n.score; });
  data.edges.forEach(function (e) { if (e.weight > maxWeight) maxWeight = e.weight; });

  var cycleEdgeSet = {};
  data.cycles.forEach(function (cycle) {
    for (var i = 0; i < cycle.length - 1; i++) {
      cycleEdgeSet[cycle[i] + '->' + cycle[i + 1]] = true;
    }
  });

  var state = {
    mode: 'svg',
    languages: {},
    edgeKinds: { Imports: true, Defines: true, Calls: true },
    collapsedCommunities: {},
    activeCycle: -1,
    highlightedNode: null,
    searchQuery: '',
    chargeStrength: -120,
    linkDistance: 80,
    centerGravity: 0.05,
    transform: d3.zoomIdentity,
    pinnedNodes: {},
    simulation: null,
    simNodes: [],
    simEdges: [],
    antOffset: 0
  };

  var langSet = {};
  data.nodes.forEach(function (n) { langSet[n.language] = true; });
  Object.keys(langSet).forEach(function (l) { state.languages[l] = true; });

  // ===========================================================================
  // Helpers
  // ===========================================================================

  function nodeRadius(n) {
    var s = n.isGroup ? n.groupScore : n.score;
    var base = MIN_R + (s / maxScore) * (MAX_R - MIN_R);
    return n.isGroup ? base * 1.5 : base;
  }

  function nodeColor(n) {
    return COMMUNITY_COLORS[n.community_id % COMMUNITY_COLORS.length];
  }

  function edgeOpacity(e) {
    return 0.3 + 0.7 * (e.weight / maxWeight);
  }

  function isInCycle(source, target) {
    return cycleEdgeSet[source + '->' + target] === true;
  }

  function shortName(id) {
    var parts = id.split('.');
    return parts[parts.length - 1];
  }

  // ===========================================================================
  // Working set — applies filters and community collapse
  // ===========================================================================

  function buildWorkingSet() {
    var groupNodes = {};
    var nodes = [];
    var communityOf = {};

    data.nodes.forEach(function (n) { communityOf[n.id] = n.community_id; });

    data.nodes.forEach(function (n) {
      if (!state.languages[n.language]) return;
      if (state.collapsedCommunities[n.community_id]) {
        if (!groupNodes[n.community_id]) {
          groupNodes[n.community_id] = {
            id: '__group_' + n.community_id,
            kind: 'Group',
            community_id: n.community_id,
            score: 0,
            groupScore: 0,
            memberCount: 0,
            isGroup: true,
            language: n.language,
            file_path: '',
            line: 0,
            is_local: true,
            betweenness: 0,
            pagerank: 0,
            in_degree: 0,
            out_degree: 0,
            in_cycle: false
          };
        }
        var g = groupNodes[n.community_id];
        if (n.score > g.groupScore) g.groupScore = n.score;
        g.memberCount++;
        return;
      }
      nodes.push(Object.assign({}, n));
    });

    Object.keys(groupNodes).forEach(function (cid) {
      var g = groupNodes[cid];
      g.label = 'C' + cid + ' (' + g.memberCount + ')';
      nodes.push(g);
    });

    var nodeIdSet = {};
    nodes.forEach(function (n) { nodeIdSet[n.id] = true; });

    var edgeMap = {};
    data.edges.forEach(function (e) {
      if (!state.edgeKinds[e.kind]) return;
      var srcId = e.source;
      var tgtId = e.target;
      var srcC = communityOf[srcId];
      var tgtC = communityOf[tgtId];
      if (srcC !== undefined && state.collapsedCommunities[srcC]) srcId = '__group_' + srcC;
      if (tgtC !== undefined && state.collapsedCommunities[tgtC]) tgtId = '__group_' + tgtC;
      if (srcId === tgtId) return;
      if (!nodeIdSet[srcId] || !nodeIdSet[tgtId]) return;
      var key = srcId + '->' + tgtId;
      if (edgeMap[key]) {
        edgeMap[key].weight += e.weight;
      } else {
        edgeMap[key] = {
          source: srcId, target: tgtId,
          kind: e.kind, weight: e.weight,
          inCycle: isInCycle(e.source, e.target)
        };
      }
    });

    var edges = Object.keys(edgeMap).map(function (k) { return edgeMap[k]; });
    return { nodes: nodes, edges: edges };
  }

  // ===========================================================================
  // Dimming logic
  // ===========================================================================

  function getNodeOpacity(n) {
    if (state.activeCycle >= 0) {
      var cycle = data.cycles[state.activeCycle];
      if (!cycle) return 1;
      return cycle.indexOf(n.id) >= 0 ? 1 : 0.1;
    }
    if (state.highlightedNode) {
      if (n.id === state.highlightedNode) return 1;
      var isNeighbor = state.simEdges.some(function (e) {
        var sid = typeof e.source === 'object' ? e.source.id : e.source;
        var tid = typeof e.target === 'object' ? e.target.id : e.target;
        return (sid === state.highlightedNode && tid === n.id) ||
               (tid === state.highlightedNode && sid === n.id);
      });
      return isNeighbor ? 1 : 0.1;
    }
    if (state.searchQuery) {
      return n.id.toLowerCase().indexOf(state.searchQuery.toLowerCase()) >= 0 ? 1 : 0.3;
    }
    return 1;
  }

  function getEdgeActive(e) {
    var sid = typeof e.source === 'object' ? e.source.id : e.source;
    var tid = typeof e.target === 'object' ? e.target.id : e.target;
    if (state.activeCycle >= 0) {
      var cycle = data.cycles[state.activeCycle];
      if (!cycle) return true;
      for (var i = 0; i < cycle.length - 1; i++) {
        if (cycle[i] === sid && cycle[i + 1] === tid) return true;
      }
      return false;
    }
    if (state.highlightedNode) {
      return sid === state.highlightedNode || tid === state.highlightedNode;
    }
    return true;
  }

  // ===========================================================================
  // Force Simulation
  // ===========================================================================

  function createSimulation() {
    var ws = buildWorkingSet();
    state.simNodes = ws.nodes;
    state.simEdges = ws.edges;

    state.simNodes.forEach(function (n) {
      if (state.pinnedNodes[n.id]) {
        n.fx = state.pinnedNodes[n.id].x;
        n.fy = state.pinnedNodes[n.id].y;
      }
    });

    state.mode = state.simNodes.length > CANVAS_THRESHOLD ? 'canvas' : 'svg';

    if (state.simulation) state.simulation.stop();

    var vp = document.getElementById('viewport');
    var cx = vp.clientWidth / 2;
    var cy = vp.clientHeight / 2;

    state.simulation = d3.forceSimulation(state.simNodes)
      .force('link', d3.forceLink(state.simEdges).id(function (d) { return d.id; }).distance(state.linkDistance))
      .force('charge', d3.forceManyBody().strength(state.chargeStrength))
      .force('center', d3.forceCenter(cx, cy).strength(state.centerGravity))
      .force('collision', d3.forceCollide().radius(function (d) { return nodeRadius(d) + 2; }))
      .alphaDecay(0.02)
      .on('tick', tick);
  }

  // ===========================================================================
  // Rendering — SVG
  // ===========================================================================

  var svgEl, svgGroup, svgLinks, svgNodes, zoomBehavior;
  var canvasEl, canvasCtx;

  function setupViewport() {
    var vp = document.getElementById('viewport');
    while (vp.firstChild) vp.removeChild(vp.firstChild);

    if (state.mode === 'svg') {
      svgEl = d3.select(vp).append('svg');
      svgGroup = svgEl.append('g');
      svgGroup.append('g').attr('class', 'links');
      svgGroup.append('g').attr('class', 'nodes');
      zoomBehavior = d3.zoom().scaleExtent([0.1, 10]).on('zoom', function (event) {
        state.transform = event.transform;
        svgGroup.attr('transform', event.transform);
      });
      svgEl.call(zoomBehavior);
      svgEl.on('dblclick.zoom', function () {
        svgEl.transition().duration(500).call(zoomBehavior.transform, d3.zoomIdentity);
      });
      svgEl.on('click', function (event) {
        if (event.target === svgEl.node()) clearHighlight();
      });
    } else {
      canvasEl = document.createElement('canvas');
      canvasEl.width = vp.clientWidth * window.devicePixelRatio;
      canvasEl.height = vp.clientHeight * window.devicePixelRatio;
      canvasEl.style.width = vp.clientWidth + 'px';
      canvasEl.style.height = vp.clientHeight + 'px';
      vp.appendChild(canvasEl);
      canvasCtx = canvasEl.getContext('2d');
      canvasCtx.scale(window.devicePixelRatio, window.devicePixelRatio);

      zoomBehavior = d3.zoom().scaleExtent([0.1, 10]).on('zoom', function (event) {
        state.transform = event.transform;
      });
      d3.select(canvasEl).call(zoomBehavior);
      d3.select(canvasEl).on('dblclick.zoom', function () {
        d3.select(canvasEl).transition().duration(500).call(zoomBehavior.transform, d3.zoomIdentity);
      });

      canvasEl.addEventListener('mousemove', function (event) {
        var rect = canvasEl.getBoundingClientRect();
        var node = findNodeAt(event.clientX - rect.left, event.clientY - rect.top);
        canvasEl.style.cursor = node ? 'pointer' : 'default';
        if (node) updateTooltip(node); else resetTooltip();
      });
      canvasEl.addEventListener('click', function (event) {
        var rect = canvasEl.getBoundingClientRect();
        var node = findNodeAt(event.clientX - rect.left, event.clientY - rect.top);
        if (node) highlightNode(node.id); else clearHighlight();
      });

      var dragTarget = null;
      d3.select(canvasEl).call(
        d3.drag()
          .subject(function (event) { return findNodeAt(event.x, event.y); })
          .on('start', function (event) {
            dragTarget = event.subject;
            if (dragTarget) {
              state.simulation.alphaTarget(0.3).restart();
              dragTarget.fx = dragTarget.x;
              dragTarget.fy = dragTarget.y;
            }
          })
          .on('drag', function (event) {
            if (dragTarget) {
              var pt = state.transform.invert([event.x, event.y]);
              dragTarget.fx = pt[0];
              dragTarget.fy = pt[1];
            }
          })
          .on('end', function () {
            if (dragTarget) {
              state.simulation.alphaTarget(0);
              state.pinnedNodes[dragTarget.id] = { x: dragTarget.fx, y: dragTarget.fy };
              dragTarget = null;
            }
          })
      );
    }
  }

  function renderSVGGraph() {
    svgLinks = svgGroup.select('.links').selectAll('.link')
      .data(state.simEdges, function (d) {
        var sid = typeof d.source === 'object' ? d.source.id : d.source;
        var tid = typeof d.target === 'object' ? d.target.id : d.target;
        return sid + '->' + tid;
      });
    svgLinks.exit().remove();
    var linkEnter = svgLinks.enter().append('line').attr('class', 'link');
    svgLinks = linkEnter.merge(svgLinks);
    svgLinks.each(function (d) {
      var el = d3.select(this);
      var color = d.inCycle ? CYCLE_COLOR : (EDGE_COLORS[d.kind] || '#999');
      el.attr('stroke', color)
        .attr('stroke-width', d.inCycle ? 2.5 : 1)
        .attr('stroke-opacity', edgeOpacity(d));
      if (EDGE_DASH[d.kind] && !d.inCycle) el.attr('stroke-dasharray', EDGE_DASH[d.kind]);
      else el.attr('stroke-dasharray', null);
    });

    svgNodes = svgGroup.select('.nodes').selectAll('.node')
      .data(state.simNodes, function (d) { return d.id; });
    svgNodes.exit().remove();
    var nodeEnter = svgNodes.enter().append('g').attr('class', 'node');
    nodeEnter.append('circle');
    nodeEnter.append('text')
      .attr('text-anchor', 'middle')
      .attr('font-size', '9px')
      .attr('fill', '#666')
      .attr('pointer-events', 'none');
    svgNodes = nodeEnter.merge(svgNodes);

    svgNodes.select('circle')
      .attr('r', function (d) { return nodeRadius(d); })
      .attr('fill', function (d) { return nodeColor(d); });

    svgNodes.select('text')
      .attr('dy', function (d) { return nodeRadius(d) + 12; })
      .text(function (d) { return d.isGroup ? d.label : shortName(d.id); });

    svgNodes.on('mouseover', function (event, d) { updateTooltip(d); })
      .on('mouseout', function () { resetTooltip(); })
      .on('click', function (event, d) {
        event.stopPropagation();
        highlightNode(d.id);
      })
      .on('dblclick', function (event, d) {
        event.stopPropagation();
        if (state.pinnedNodes[d.id]) {
          delete state.pinnedNodes[d.id];
          d.fx = null; d.fy = null;
          state.simulation.alpha(0.3).restart();
        }
      });

    svgNodes.call(d3.drag()
      .on('start', function (event, d) {
        state.simulation.alphaTarget(0.3).restart();
        d.fx = d.x; d.fy = d.y;
      })
      .on('drag', function (event, d) {
        d.fx = event.x; d.fy = event.y;
      })
      .on('end', function (event, d) {
        state.simulation.alphaTarget(0);
        state.pinnedNodes[d.id] = { x: d.fx, y: d.fy };
      })
    );
  }

  // ===========================================================================
  // Rendering — Canvas
  // ===========================================================================

  function renderCanvasFrame() {
    if (state.mode !== 'canvas' || !canvasCtx) return;
    var w = canvasEl.width / window.devicePixelRatio;
    var h = canvasEl.height / window.devicePixelRatio;
    var ctx = canvasCtx;

    ctx.save();
    ctx.setTransform(window.devicePixelRatio, 0, 0, window.devicePixelRatio, 0, 0);
    ctx.clearRect(0, 0, w, h);
    ctx.translate(state.transform.x, state.transform.y);
    ctx.scale(state.transform.k, state.transform.k);

    state.simEdges.forEach(function (e) {
      if (!e.source.x) return;
      var active = getEdgeActive(e);
      ctx.beginPath();
      ctx.moveTo(e.source.x, e.source.y);
      ctx.lineTo(e.target.x, e.target.y);
      ctx.strokeStyle = e.inCycle ? CYCLE_COLOR : (EDGE_COLORS[e.kind] || '#999');
      ctx.lineWidth = e.inCycle ? 2.5 : 1;
      ctx.globalAlpha = active ? edgeOpacity(e) : 0.05;

      if (state.activeCycle >= 0 && e.inCycle && active) {
        ctx.setLineDash([10, 5]);
        ctx.lineDashOffset = -state.antOffset;
        ctx.lineWidth = 3;
        ctx.globalAlpha = 1;
      } else if (EDGE_DASH[e.kind]) {
        ctx.setLineDash(EDGE_DASH[e.kind].split(',').map(Number));
      } else {
        ctx.setLineDash([]);
      }
      ctx.stroke();
      ctx.setLineDash([]);
      ctx.globalAlpha = 1;
    });

    state.simNodes.forEach(function (n) {
      if (n.x === undefined) return;
      var r = nodeRadius(n);
      ctx.globalAlpha = getNodeOpacity(n);
      ctx.beginPath();
      ctx.arc(n.x, n.y, r, 0, 2 * Math.PI);
      ctx.fillStyle = nodeColor(n);
      ctx.fill();
      ctx.strokeStyle = '#fff';
      ctx.lineWidth = 1.5;
      ctx.stroke();
    });
    ctx.globalAlpha = 1;
    ctx.restore();
  }

  function findNodeAt(screenX, screenY) {
    var pt = state.transform.invert([screenX, screenY]);
    var x = pt[0], y = pt[1];
    for (var i = state.simNodes.length - 1; i >= 0; i--) {
      var n = state.simNodes[i];
      if (n.x === undefined) continue;
      if (Math.hypot(n.x - x, n.y - y) <= nodeRadius(n)) return n;
    }
    return null;
  }

  // ===========================================================================
  // Tick
  // ===========================================================================

  function tick() {
    if (state.mode === 'svg') {
      svgLinks
        .attr('x1', function (d) { return d.source.x; })
        .attr('y1', function (d) { return d.source.y; })
        .attr('x2', function (d) { return d.target.x; })
        .attr('y2', function (d) { return d.target.y; });
      svgNodes.attr('transform', function (d) { return 'translate(' + d.x + ',' + d.y + ')'; });
      svgNodes.each(function (d) {
        d3.select(this).select('circle').attr('opacity', getNodeOpacity(d));
        d3.select(this).select('text').attr('opacity', getNodeOpacity(d));
      });
      svgLinks.each(function (d) {
        var active = getEdgeActive(d);
        d3.select(this).classed('dimmed', !active);
        d3.select(this).classed('marching-ants', state.activeCycle >= 0 && d.inCycle && active);
      });
    } else {
      if (state.activeCycle >= 0) state.antOffset = (state.antOffset + 0.5) % 15;
      renderCanvasFrame();
    }
  }

  // ===========================================================================
  // Sidebar — safe DOM construction (no innerHTML)
  // ===========================================================================

  function renderSidebar() {
    var sb = document.getElementById('sidebar');
    while (sb.firstChild) sb.removeChild(sb.firstChild);
    renderSummary(sb);
    renderFilters(sb);
    renderCommunities(sb);
    renderCycles(sb);
    renderForceControls(sb);
    renderSearch(sb);
  }

  function makeSection(parent, title, collapsed) {
    var sec = document.createElement('div');
    sec.className = 'section';
    var hdr = document.createElement('div');
    hdr.className = 'section-header' + (collapsed ? ' collapsed' : '');
    hdr.textContent = title;
    var content = document.createElement('div');
    content.className = 'section-content' + (collapsed ? ' hidden' : '');
    hdr.addEventListener('click', function () {
      hdr.classList.toggle('collapsed');
      content.classList.toggle('hidden');
    });
    sec.appendChild(hdr);
    sec.appendChild(content);
    parent.appendChild(sec);
    return content;
  }

  function renderSummary(sb) {
    var content = makeSection(sb, 'Summary', false);
    var grid = document.createElement('div');
    grid.className = 'summary-grid';
    var items = [
      { value: data.summary.total_nodes, label: 'Nodes' },
      { value: data.summary.total_edges, label: 'Edges' },
      { value: data.summary.total_communities, label: 'Communities' },
      { value: data.summary.total_cycles, label: 'Cycles' }
    ];
    items.forEach(function (item) {
      var el = document.createElement('div');
      el.className = 'summary-item';
      var valDiv = document.createElement('div');
      valDiv.className = 'summary-value';
      valDiv.textContent = item.value;
      var lblDiv = document.createElement('div');
      lblDiv.className = 'summary-label';
      lblDiv.textContent = item.label;
      el.appendChild(valDiv);
      el.appendChild(lblDiv);
      grid.appendChild(el);
    });
    content.appendChild(grid);
  }

  function renderFilters(sb) {
    var content = makeSection(sb, 'Filters', false);

    var langGroup = document.createElement('div');
    langGroup.className = 'filter-group';
    var langLabel = document.createElement('div');
    langLabel.className = 'filter-group-label';
    langLabel.textContent = 'Language';
    langGroup.appendChild(langLabel);
    Object.keys(state.languages).forEach(function (lang) {
      var item = document.createElement('label');
      item.className = 'filter-item';
      var cb = document.createElement('input');
      cb.type = 'checkbox';
      cb.checked = state.languages[lang];
      cb.addEventListener('change', function () {
        state.languages[lang] = cb.checked;
        rebuild();
      });
      item.appendChild(cb);
      item.appendChild(document.createTextNode(lang));
      langGroup.appendChild(item);
    });
    content.appendChild(langGroup);

    var edgeGroup = document.createElement('div');
    edgeGroup.className = 'filter-group';
    var edgeLabel = document.createElement('div');
    edgeLabel.className = 'filter-group-label';
    edgeLabel.textContent = 'Edge Type';
    edgeGroup.appendChild(edgeLabel);
    ['Imports', 'Defines', 'Calls'].forEach(function (kind) {
      var item = document.createElement('label');
      item.className = 'filter-item';
      var cb = document.createElement('input');
      cb.type = 'checkbox';
      cb.checked = state.edgeKinds[kind];
      cb.addEventListener('change', function () {
        state.edgeKinds[kind] = cb.checked;
        rebuild();
      });
      item.appendChild(cb);
      item.appendChild(document.createTextNode(kind));
      edgeGroup.appendChild(item);
    });
    content.appendChild(edgeGroup);
  }

  function renderCommunities(sb) {
    var content = makeSection(sb, 'Communities', data.communities.length > 10);
    data.communities.forEach(function (c) {
      var item = document.createElement('div');
      item.className = 'community-item';
      if (state.collapsedCommunities[c.id]) item.classList.add('collapsed-community');
      var dot = document.createElement('span');
      dot.className = 'community-dot';
      dot.style.background = COMMUNITY_COLORS[c.id % COMMUNITY_COLORS.length];
      item.appendChild(dot);
      item.appendChild(document.createTextNode('C' + c.id + ' (' + c.members.length + ')'));
      item.addEventListener('click', function () {
        if (state.collapsedCommunities[c.id]) delete state.collapsedCommunities[c.id];
        else state.collapsedCommunities[c.id] = true;
        rebuild();
      });
      content.appendChild(item);
    });
  }

  function renderCycles(sb) {
    if (data.cycles.length === 0) return;
    var content = makeSection(sb, 'Cycles (' + data.cycles.length + ')', data.cycles.length > 10);
    data.cycles.forEach(function (cycle, idx) {
      var item = document.createElement('div');
      item.className = 'cycle-item';
      if (state.activeCycle === idx) item.classList.add('active');
      var chain = cycle.map(shortName).join(' \u2192 ');
      item.textContent = (idx + 1) + '. ' + chain;
      item.title = cycle.join(' \u2192 ');
      item.addEventListener('click', function () {
        if (state.activeCycle === idx) { clearHighlight(); }
        else {
          state.activeCycle = idx;
          state.highlightedNode = null;
          zoomToCycle(cycle);
          renderSidebar();
        }
      });
      content.appendChild(item);
    });
  }

  function renderForceControls(sb) {
    var content = makeSection(sb, 'Force Controls', true);
    makeSlider(content, 'Charge', state.chargeStrength, -300, -10, 1, function (v) {
      state.chargeStrength = v;
      state.simulation.force('charge').strength(v);
      state.simulation.alpha(0.3).restart();
    });
    makeSlider(content, 'Link Distance', state.linkDistance, 20, 300, 1, function (v) {
      state.linkDistance = v;
      state.simulation.force('link').distance(v);
      state.simulation.alpha(0.3).restart();
    });
    makeSlider(content, 'Gravity', state.centerGravity, 0, 0.3, 0.01, function (v) {
      state.centerGravity = v;
      state.simulation.force('center').strength(v);
      state.simulation.alpha(0.3).restart();
    });
  }

  function makeSlider(parent, label, value, min, max, step, onChange) {
    var group = document.createElement('div');
    group.className = 'slider-group';
    var lbl = document.createElement('div');
    lbl.className = 'slider-label';
    var nameSpan = document.createElement('span');
    nameSpan.textContent = label;
    var valueSpan = document.createElement('span');
    valueSpan.textContent = String(value);
    lbl.appendChild(nameSpan);
    lbl.appendChild(valueSpan);
    var input = document.createElement('input');
    input.type = 'range';
    input.min = String(min);
    input.max = String(max);
    input.step = String(step);
    input.value = String(value);
    input.addEventListener('input', function () {
      var v = parseFloat(input.value);
      valueSpan.textContent = String(v);
      onChange(v);
    });
    group.appendChild(lbl);
    group.appendChild(input);
    parent.appendChild(group);
  }

  function renderSearch(sb) {
    var content = makeSection(sb, 'Search', false);
    var input = document.createElement('input');
    input.type = 'text';
    input.id = 'search-input';
    input.placeholder = 'Search modules...';
    input.value = state.searchQuery;
    var debounceTimer;
    input.addEventListener('input', function () {
      clearTimeout(debounceTimer);
      debounceTimer = setTimeout(function () {
        state.searchQuery = input.value;
        if (state.mode === 'svg') tick();
        if (state.searchQuery) {
          var matches = state.simNodes.filter(function (n) {
            return n.id.toLowerCase().indexOf(state.searchQuery.toLowerCase()) >= 0;
          });
          if (matches.length === 1 && matches[0].x !== undefined) zoomToNode(matches[0]);
        }
      }, 200);
    });
    content.appendChild(input);
  }

  // ===========================================================================
  // Highlight / Zoom helpers
  // ===========================================================================

  function highlightNode(id) {
    state.highlightedNode = id;
    state.activeCycle = -1;
    if (state.mode === 'svg') tick();
    renderSidebar();
  }

  function clearHighlight() {
    state.highlightedNode = null;
    state.activeCycle = -1;
    if (state.mode === 'svg') tick();
    renderSidebar();
  }

  function zoomToCycle(cycle) {
    var nodes = state.simNodes.filter(function (n) { return cycle.indexOf(n.id) >= 0; });
    if (nodes.length > 0) zoomToNodes(nodes);
  }

  function zoomToNode(node) {
    var vp = document.getElementById('viewport');
    var w = vp.clientWidth, h = vp.clientHeight;
    var scale = 1.5;
    var t = d3.zoomIdentity.translate(w / 2 - node.x * scale, h / 2 - node.y * scale).scale(scale);
    var target = state.mode === 'svg' ? svgEl : d3.select(canvasEl);
    target.transition().duration(500).call(zoomBehavior.transform, t);
  }

  function zoomToNodes(nodes) {
    if (nodes.length === 0) return;
    var vp = document.getElementById('viewport');
    var w = vp.clientWidth, h = vp.clientHeight;
    var x0 = Infinity, y0 = Infinity, x1 = -Infinity, y1 = -Infinity;
    nodes.forEach(function (n) {
      if (n.x < x0) x0 = n.x; if (n.y < y0) y0 = n.y;
      if (n.x > x1) x1 = n.x; if (n.y > y1) y1 = n.y;
    });
    var pad = 60;
    var dx = (x1 - x0) + pad * 2, dy = (y1 - y0) + pad * 2;
    var cx = (x0 + x1) / 2, cy = (y0 + y1) / 2;
    var scale = Math.min(w / dx, h / dy, 3);
    var t = d3.zoomIdentity.translate(w / 2 - cx * scale, h / 2 - cy * scale).scale(scale);
    var target = state.mode === 'svg' ? svgEl : d3.select(canvasEl);
    target.transition().duration(500).call(zoomBehavior.transform, t);
  }

  // ===========================================================================
  // Tooltip
  // ===========================================================================

  function updateTooltip(n) {
    var tip = document.getElementById('tooltip');
    if (n.isGroup) {
      tip.textContent = n.label + ' | Community ' + n.community_id;
      return;
    }
    tip.textContent = n.id +
      ' | ' + n.kind +
      ' | Score: ' + n.score.toFixed(4) +
      ' | BT: ' + n.betweenness.toFixed(4) +
      ' | PR: ' + n.pagerank.toFixed(4) +
      ' | In: ' + n.in_degree +
      ' | Out: ' + n.out_degree +
      ' | Community: ' + n.community_id +
      (n.in_cycle ? ' | IN CYCLE' : '');
  }

  function resetTooltip() {
    document.getElementById('tooltip').textContent = 'Hover over a node to see details';
  }

  // ===========================================================================
  // PNG Export
  // ===========================================================================

  function exportPNG() {
    var filename = data.project_name + '-graph.png';
    if (state.mode === 'canvas') {
      var link = document.createElement('a');
      link.download = filename;
      link.href = canvasEl.toDataURL('image/png');
      link.click();
    } else {
      var svgData = new XMLSerializer().serializeToString(svgEl.node());
      var img = new Image();
      var svgBlob = new Blob([svgData], { type: 'image/svg+xml;charset=utf-8' });
      var url = URL.createObjectURL(svgBlob);
      img.onload = function () {
        var c = document.createElement('canvas');
        var vp = document.getElementById('viewport');
        c.width = vp.clientWidth * 2;
        c.height = vp.clientHeight * 2;
        var cx = c.getContext('2d');
        cx.scale(2, 2);
        cx.fillStyle = '#f8f9fa';
        cx.fillRect(0, 0, vp.clientWidth, vp.clientHeight);
        cx.drawImage(img, 0, 0);
        var dl = document.createElement('a');
        dl.download = filename;
        dl.href = c.toDataURL('image/png');
        dl.click();
        URL.revokeObjectURL(url);
      };
      img.src = url;
    }
  }

  // ===========================================================================
  // Rebuild (on filter/collapse change)
  // ===========================================================================

  function rebuild() {
    clearHighlight();
    createSimulation();
    setupViewport();
    if (state.mode === 'svg') renderSVGGraph();
    renderSidebar();
  }

  // ===========================================================================
  // Init
  // ===========================================================================

  function init() {
    document.getElementById('project-name').textContent = data.project_name;
    document.getElementById('export-png').addEventListener('click', exportPNG);

    if (data.nodes.length === 0) {
      var vp = document.getElementById('viewport');
      var empty = document.createElement('div');
      empty.className = 'empty-state';
      empty.textContent = 'No nodes to visualize';
      vp.appendChild(empty);
      renderSidebar();
      return;
    }

    createSimulation();
    setupViewport();
    if (state.mode === 'svg') renderSVGGraph();
    renderSidebar();
  }

  document.addEventListener('DOMContentLoaded', init);
})();
