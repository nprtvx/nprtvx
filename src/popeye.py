# ## ###
style = f"""
.popeye {{
background-color: #8926;
}}
"""
# ## ###
script = f"""
const popeye = document.getElemenetById('popeye');
popeye.classList.add('popeye');
"""
# ## ###
popeye = f"""
<div id='popeye'></div>
<style>
{style}
</style>
<script>
{script}
</script>
"""
