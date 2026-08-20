// Shared canvas sparkline renderer used by system-monitor.js and
// network.js, so both just manage their own history arrays and call this.

/**
 * @param {HTMLCanvasElement} canvas
 * @param {{ values: number[], color: string }[]} series
 * @param {{ max?: number, length?: number }} [options]
 */
export function drawSparkline(canvas, series, options = {}) {
  const ctx = canvas.getContext("2d");
  const scale = window.devicePixelRatio || 1;
  const cssWidth = canvas.clientWidth || canvas.width;
  const cssHeight = canvas.clientHeight || canvas.height;

  if (canvas.width !== cssWidth * scale || canvas.height !== cssHeight * scale) {
    canvas.width = cssWidth * scale;
    canvas.height = cssHeight * scale;
  }

  const width = canvas.width;
  const height = canvas.height;
  ctx.clearRect(0, 0, width, height);

  const allValues = series.flatMap((s) => s.values);
  const maxValue = options.max ?? Math.max(1, ...allValues);
  const length = options.length ?? Math.max(2, ...series.map((s) => s.values.length));

  for (const { values, color } of series) {
    if (values.length < 2) continue;
    ctx.beginPath();
    ctx.strokeStyle = color;
    ctx.lineWidth = 1.5 * scale;
    ctx.lineJoin = "round";
    values.forEach((value, index) => {
      const x = (index / (length - 1)) * width;
      const y = height - (value / maxValue) * (height - 4 * scale) - 2 * scale;
      if (index === 0) ctx.moveTo(x, y);
      else ctx.lineTo(x, y);
    });
    ctx.stroke();
  }
}
