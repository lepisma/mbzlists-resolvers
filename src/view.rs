pub fn generate_page(body: &str) -> String {
    format!("<!DOCTYPE html>
<html lang=\"en\">
<head>
  <meta charset=\"UTF-8\">
  <title>mbzlist-resolvers</title>
  <style>
    body {{
      font-family: monospace;
      font-size: 16px;
      line-height: 1.6;
      margin: 2rem;
      background: #f4f4f4;
      color: #333;
    }}
    h1 {{
      font-size: 2rem;
      margin-bottom: 1rem;
    }}
    h2 {{
      font-size: 1.25rem;
      margin-bottom: 0.5rem;
    }}
    p {{
      margin-bottom: 1rem;
      color: #555;
    }}
    .btn {{
      background: #444;
      color: white;
      border: none;
      padding: 0.5rem 1rem;
      border-radius: 4px;
      cursor: pointer;
      font-family: inherit;
      text-decoration: none;
    }}
    .btn:hover {{
      background: #222;
    }}
    .card {{
      background: #e0e0e0;
      border-radius: 8px;
      padding: 1.5rem;
      margin-bottom: 1.5rem;
      box-shadow: 0 2px 5px rgba(0,0,0,0.05);
    }}
  </style>
</head>
<body>
  {body}
</body>
</html>")
}
