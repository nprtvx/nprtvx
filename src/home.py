


style = f"""
html, body {{
background-image: linear-gradient(#000d, #000d);
}}
.home {{
position: absolute;
top: 0;
bottom: 0;
right: 0;
left: 0;
box-shadow: 0 0 0 4rem #8926 inset;
display: flex;
align-items: center;
justify-content: center;
flex-direction: column;
background-image: url("https://images.pexels.com/photos/3311574/pexels-photo-3311574.jpeg?auto=compress&cs=tinysrgb&w=1260&h=750&dpr=1");
background-size: cover;
background-position: center;
}}

.home h1 {{
position: absolute;
top: 26px;
right: 26px;
font-size: 72px;
color: #892C;
text-align: center;
}}
"""

script = f"""
const home = document.getElementById('home');
home.classList.add('home');
"""

home = f"""
<div id='home'>
<h1>nprtvx</h1>
</div>
<style>
{style}
</style>
<script>
{script}
</script>
"""
