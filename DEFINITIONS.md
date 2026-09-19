# AXIAM demo - IoT domotic properties management

The aim of this document is to describe the structure of a small demo to showcase AXIAM IAM managing different tenants and sites with IoT devices, property managers and local users.

## Structure of the property
Each property manager is a separate tenant in AXIAM.
Each tenant contains all the sites managed by the property managers; each site groups all its related buildings and each building is a group of apartments.
IoT devices can be linked at site, building or apartment level.

## Type of users
There are mainly 4 groups of users:
- **Property managers**: Users managing the properties
- **Installers**: Users that works for the property managers to setup the IoT devices in their porperties
- **Concierges**: Users that works as concierge in one or more sites
- **Users**: People living in the apartments

## IoT devices
IoT devices belongs to 3 categories:
- **Intercoms outdoors**: Devices linked to sites gates or buildings doors that allow visitors to call _intercom indoors_ in one apartment and unlock the gate/door (supported actions: call an indoor intercom)
- **Intercoms indoors**: Device linked ot one apartment those can receives calls from one ore more _intercom outdoors_ (supported actions: answer a call made by an outdoor intercom)
- **Lights**: Sites, buildings or apartment lights (supported actions turn on/off, dim lights, cahnge RGB colors)
- **Thermostats**: device controlling heating/cooling of an apartment (supported actions: turn on/off, change mode between heating/cooling/ set the desired temperature)

## Permissions

### Property managers
- can admin sites, buildings and apartments
  - create new sites
  - create new buildings in a site
  - create apartments in a building
  - add users (installers) to sites and apartments
  - add user (local users) to site, building and apartments
- can edit sites, buildings and apartments
- can delete sites, buildings and apartments
- can add/edit/delete devices in sites and buildings
- can't operate on devices inside apartments

### Installers
- can add/edit/delete devices in sites, building and apartments
- can configure devices

### Concierges
- can operate on devices in sites or buildings 

### Local users
- can add/edit/delete devices in their apartment

### IoT devices
- can interact with the IoT platform services in order to keep their status shadow copy updated
- must receive commands

## IoT Platform
The platform has 4 main components:
- **AXIAM**, as centralized IAM
- **Management Platform**, a Java microservice that manages the property allowing users to perform the required actions to setup, configure and mantain the porperties
- **Device Twin**, a Rust microservice that keep the shadow copy of the devices status and provide the enpoints to send them commands
- **Frontend**, a React frontend that allow users to operate
_Managemnt Platform_, _Device Twin_ and _Frontend_ interacts with AXIAM using SDKs (for react prefer WASM wherever possible); theu use gRPC for the calls to AXIAM and they're connected to it using mTLS if possible

## IoT Devices
IoT devices are simulators, answering to all the calls made by the IoT platform, keeping their status consistent with platform shadow copy and simulating their behavior (e.g. the heating of a room).
They are built in C, C++ and Rust using AXIAM SDKs to manage their logins, they use mTLS and service accounts.

## Further constraints
- a local user can operate devices in his own apartment or in the building/site to which his apartments belongs to
- property managers, concierges and installers can perform actions only on sites or buildings related device, they can never operate on apartment devices; the only excpetion is for installer those might be explictly enabled to operate on one or more apartment devices by one of the partment users

## Deliverables
- _Managemnt Platform_, _Device Twin_ and _Frontend_ code, tests and documentation
- setup scripts and instructions
- scripts to seed with at least 2 properties with sites, buildings, devices and users of different types
- a small fleet of device simulator, with scripts and instructions to deploy them:
  - 1 outdoor intercom per site
  - 1 outdoor intercom per building
  - 3 lights per building
  - 1 indoor intercom per apartment
  - 3 lights per apartment
  - 2 thermostats per apartment
- consider a site to have at least 2 buildings with 4 apartments inside it
- for users consider to have:
  - at least one property manager per tenant
  - at least one installer per tenant
  - at least one concierge per site
  - at least 2 users per apartment
- add at least 2 tenants
