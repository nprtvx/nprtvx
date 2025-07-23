


style = f"""
* {{
margin: 0;
padding: 0;
box-sizing: border-box;
}}

html, body {{
background-color: #000d;
}}

.home {{
position: absolute;
top: 0;
bottom: 0;
right: 0;
left: 0;
box-shadow: 0 0 0 100rem #8926 inset;
display: flex;
align-items: center;
justify-content: center;
flex-direction: column;
background-image: url("https://images.pexels.com/photos/3311574/pexels-photo-3311574.jpeg?auto=compress&cs=tinysrgb&w=1260&h=750&dpr=1");
background-size: cover;
background-position: center;
}}

.home h1 {{
font-size: 8rem;
color: #9008;
text-align: center;
text-transform: uppercase;
font-weight: 400;
border: 8px 16px solid #9008;
font-family: system-ui, Arial, sans-serif;
}}
"""

script = f"""
const home = document.getElementById('home');
home.classList.add('home');
const aBtn2shoot = document.createElement('div');
btn2shoot.setAttribute('id', 'btn2shoot');
btn2shoot.classList.add('btn2shoot');

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
