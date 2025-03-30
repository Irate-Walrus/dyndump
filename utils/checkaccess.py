import argparse
import json
import os

PRIVILEGES_JSON_CACHE = None

def load_json(file_path):
    """Load JSON data from a file."""
    with open(file_path, 'r') as file:
        return json.load(file)

def get_user_rolememberships(systemuserid, roles_file):
    """Retrieve user role memberships from the roles JSON file."""
    roles_data = load_json(roles_file)['value']
    return [role['roleid'] for role in roles_data if role['systemuserid'] == systemuserid]

def get_team_roles(teamid, teamroles_file):
    """Retrieve user roles from the roles JSON file."""
    teamroles_data = load_json(teamroles_file)['value']
    return [role['roleid'] for role in teamroles_data if role['teamid'] == teamid]

def get_user_teammemberships(systemuserid, teammemberships_file):
    """Retrieve teams the user is a member of from the teams JSON file."""
    teammemberships_data = load_json(teammemberships_file)['value']
    return [team['teamid'] for team in teammemberships_data if team['systemuserid'] == systemuserid]

def get_role_privileges(roleid, roleprivileges_file):
    """Retrieve role privileges from the role privileges JSON file."""
    roleprivileges_data = load_json(roleprivileges_file)['value']
    return [privilege for privilege in roleprivileges_data if privilege['roleid'] == roleid]


def get_privileges(privilegeid, privileges_file):
    """Retrieve privileges from the privileges JSON file."""
    global PRIVILEGES_JSON_CACHE
    if PRIVILEGES_JSON_CACHE is None:
        PRIVILEGES_JSON_CACHE = load_json(privileges_file)['value']
    return [privilege for privilege in PRIVILEGES_JSON_CACHE if privilege['privilegeid'] == privilegeid]

def get_team(teamid, teams_file):
    """Retrieve teams the user is a member of from the teams JSON file."""
    teams_data = load_json(teams_file)['value']
    return [team for team in teams_data if team['teamid'] == teamid]

def get_role(roleid, roles_file):
    """Retrieve role from the roles JSON file."""
    roles_data = load_json(roles_file)['value']
    return [role for role in roles_data if role['roleid'] == roleid]

def check_access(entity_set, systemuserid):
    """Check access level for a given entity set and user."""
    systemuserrolescollection_file = "systemuserrolescollection.json"
    roleprivilegescollection_file = "roleprivilegescollection.json"
    teammemberships_file = "teammemberships.json"
    teamrolescollection_file = "teamrolescollection.json"
    teams_file = "teams.json"
    privileges_file = "privileges.json"
    roles_file = "roles.json"
    entity_file = f"{entity_set}.json"
    

    if not os.path.exists(entity_file):
        print(f"File {entity_file} does not exist.")
        return

    user_rolememberships = get_user_rolememberships(systemuserid, systemuserrolescollection_file)
    user_teammemberships = get_user_teammemberships(systemuserid, teammemberships_file)


    user_roles = []
    for roleid in user_rolememberships:
        user_roles.extend(get_role(roleid, roles_file))

    user_teams = []
    for teamid in user_teammemberships:
        user_teams.extend(get_team(teamid, teams_file))

    print(f"[+] user roles: {[role['name'] for role in user_roles]}")
    print(f"[+] user teams: {[team['name'] for team in user_teams]}")


    user_roleprivileges = []
    team_roleprivileges = []
    
    
    for roleid in user_rolememberships:
        role = get_role(roleid, roles_file)[0]
        print(f"[+] user role {role['name']} privileges:")
        roleprivileges = get_role_privileges(roleid, roleprivilegescollection_file)
        for roleprivilege in roleprivileges:
            privileges = get_privileges(roleprivilege['privilegeid'], privileges_file)
            for privilege in privileges:
                print(f"[+]\t{privilege['name']}")

        user_roleprivileges.extend(get_role_privileges(roleid, roleprivilegescollection_file))
    
    for teamid in user_teammemberships:
        team = get_team(teamid, teams_file)[0]
        print(f"[+] user team {team['name']} roles:")
        roleids = get_team_roles(teamid, teamrolescollection_file)
        for roleid in roleids:
            role = get_role(roleid, roles_file)[0]
            print(f"[+]\tuser team {team['name']} role {role['name']} privileges:")
            roleprivileges = get_role_privileges(roleid, roleprivilegescollection_file)
            for roleprivilege in roleprivileges:
                privileges = get_privileges(roleprivilege['privilegeid'], privileges_file)
                for privilege in privileges:
                    print(f"[+]\t\t{privilege['name']}")
        roleids = get_team_roles(teamid, teamrolescollection_file)
        for roleid in roleids:
            team_roleprivileges.extend(get_role_privileges(roleid, roleprivilegescollection_file))
    

    #print(f"[+] user role privileges: {user_roleprivileges}")
    #print(f"[+] team role privileges: {team_roleprivileges}")

    user_privileges = []
    for roleprivilege in user_roleprivileges:
        user_privileges.extend(get_privileges(roleprivilege['privilegeid'], privileges_file))

    team_privileges = []
    for roleprivilege in team_roleprivileges:
        team_privileges.extend(get_privileges(roleprivilege['privilegeid'], privileges_file))

    #print(f"[+] user privileges:")
    #for privilege in user_privileges:
    #    print(f"[+] {privilege['name']}")

    #print(f"[+] team privileges:")
    #for privilege in team_privileges:
    #    print(f"[+] {privilege['name']}")
    
    access_level = "None"

    # Determine the business unit of the record
    #dunno ?

     # Check if the record belongs to the user or to a team the user is a member of
    if any(privilege['entity_name'] == entity_set and privilege['access_level'] == 'User' for privilege in user_roleprivileges):
        access_level = 'User'

def main():
    parser = argparse.ArgumentParser(description="Check access level for a given entity set and user.")
    parser.add_argument("entity_set", type=str, help="The name of the entity set.")
    parser.add_argument("systemuserid", type=str, help="The user ID to check access for.")

    args = parser.parse_args()

    check_access(args.entity_set, args.systemuserid)

if __name__ == "__main__":
    main()
