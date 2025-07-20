from flask import Flask

app = Flask(__name__)

@app.route('/')
def index():
  return f"""
    <div class='popeye' id='popeye'></div>
    <style>
      .popeye {{
        background-color: #8926;
        padding: 2rem;
      }}
    </style>
    <script>
      const popeye = document.getElemenetById('popeye');
      document.append(popeye*8)
    </script>
  """
